//! Message dispatch — routes GameMessage to handlers based on session state.

use std::collections::HashMap;
use std::sync::Arc;

use sidequest_agents::orchestrator::TurnContext;
use sidequest_game::builder::CharacterBuilder;
use sidequest_genre::{GenreCode, GenreLoader};
use sidequest_protocol::{
    AudioCuePayload, ChapterMarkerPayload, CharacterCreationPayload, CharacterSheetPayload,
    CharacterState, CombatEventPayload, GameMessage, InitialState, InventoryPayload,
    MapUpdatePayload, NarrationEndPayload, NarrationPayload, PartyMember, PartyStatusPayload,
    SessionEventPayload, ThinkingPayload, TurnStatusPayload,
};

use crate::helpers::narration::{
    extract_items_from_narration, extract_item_losses, extract_location_header,
    strip_location_header, strip_markdown_for_tts,
};
use crate::helpers::npc::{
    build_name_bank_context, build_npc_registry_context, update_npc_registry, NpcRegistryEntry,
};
use crate::shared_session;
use crate::telemetry::{Severity, WatcherEvent, WatcherEventType};
use crate::types::{error_response, reconnect_required_response};
use crate::{AppState, Session};

// ---------------------------------------------------------------------------
// Message dispatch
// ---------------------------------------------------------------------------

/// Dispatch a deserialized GameMessage through the session state machine.
/// Returns a list of response messages to send back to the client.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_message(
    msg: GameMessage,
    session: &mut Session,
    builder: &mut Option<CharacterBuilder>,
    player_name_store: &mut Option<String>,
    character_json: &mut Option<serde_json::Value>,
    character_name: &mut Option<String>,
    character_hp: &mut i32,
    character_max_hp: &mut i32,
    character_level: &mut u32,
    character_xp: &mut u32,
    current_location: &mut String,
    inventory: &mut sidequest_game::Inventory,
    combat_state: &mut sidequest_game::combat::CombatState,
    chase_state: &mut Option<sidequest_game::ChaseState>,
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    trope_defs: &mut Vec<sidequest_genre::TropeDefinition>,
    world_context: &mut String,
    axes_config: &mut Option<sidequest_genre::AxesConfig>,
    axis_values: &mut Vec<sidequest_game::axis::AxisValue>,
    visual_style: &mut Option<sidequest_genre::VisualStyle>,
    music_director: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_scheduler: &std::sync::Arc<
        tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>,
    >,
    npc_registry: &mut Vec<NpcRegistryEntry>,
    quest_log: &mut std::collections::HashMap<String, String>,
    narration_history: &mut Vec<String>,
    discovered_regions: &mut Vec<String>,
    turn_manager: &mut sidequest_game::TurnManager,
    lore_store: &mut sidequest_game::LoreStore,
    shared_session_holder: &Arc<
        tokio::sync::Mutex<Option<Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>,
    >,
    state: &AppState,
    player_id: &str,
    continuity_corrections: &mut String,
) -> Vec<GameMessage> {
    match &msg {
        GameMessage::SessionEvent { payload, .. } if payload.event == "connect" => {
            let mut responses = dispatch_connect(
                payload,
                session,
                builder,
                player_name_store,
                character_json,
                character_name,
                character_hp,
                character_max_hp,
                character_level,
                character_xp,
                current_location,
                discovered_regions,
                trope_defs,
                trope_states,
                world_context,
                axes_config,
                axis_values,
                visual_style,
                music_director,
                audio_mixer,
                prerender_scheduler,
                turn_manager,
                npc_registry,
                quest_log,
                lore_store,
                state,
                player_id,
                continuity_corrections,
            )
            .await;
            // After connect identifies genre/world, join/create the shared session
            if let (Some(genre), Some(world)) = (session.genre_slug(), session.world_slug()) {
                let ss = state.get_or_create_session(genre, world);
                *shared_session_holder.lock().await = Some(ss.clone());

                // Load cartography regions if not already loaded
                {
                    let mut ss_guard = ss.lock().await;
                    if ss_guard.region_names.is_empty() {
                        if let Ok(genre_code) = GenreCode::new(genre) {
                            let loader =
                                GenreLoader::new(vec![state.genre_packs_path().to_path_buf()]);
                            if let Ok(pack) = loader.load(&genre_code) {
                                if let Some(w) = pack.worlds.get(world) {
                                    ss_guard.load_cartography(&w.cartography.regions);
                                }
                            }
                        }
                    }
                }

                // If this is a returning player (already Playing), add them to
                // the shared session now. New players get added after character
                // creation completes in dispatch_character_creation.
                if session.is_playing() {
                    let mut ss_guard = ss.lock().await;
                    let pname = player_name_store
                        .clone()
                        .unwrap_or_else(|| "Player".to_string());
                    let mut ps = shared_session::PlayerState::new(pname);
                    // Populate character data from locals (set by dispatch_connect)
                    ps.character_name = character_name.clone();
                    ps.character_hp = *character_hp;
                    ps.character_max_hp = *character_max_hp;
                    ps.display_location = current_location.clone();
                    ps.region_id = ss_guard
                        .resolve_region(current_location)
                        .unwrap_or_default();
                    // Extract level/class from character_json since dispatch_connect
                    // doesn't restore them to the scalar locals.
                    if let Some(ref cj) = *character_json {
                        ps.character_level = cj
                            .get("core")
                            .and_then(|c| c.get("level"))
                            .and_then(|l| l.as_u64())
                            .unwrap_or(1) as u32;
                        ps.character_class = cj
                            .get("char_class")
                            .and_then(|c| c.as_str())
                            .unwrap_or("")
                            .to_string();
                        // Also fix the scalar locals so dispatch_player_action has them
                        *character_level = ps.character_level;
                    }
                    ss_guard.players.insert(player_id.to_string(), ps);

                    // Transition turn mode (PlayerJoined)
                    let pc = ss_guard.player_count();
                    let old_mode = std::mem::take(&mut ss_guard.turn_mode);
                    ss_guard.turn_mode = old_mode.apply(
                        sidequest_game::turn_mode::TurnModeTransition::PlayerJoined {
                            player_count: pc,
                        },
                    );
                    tracing::info!(
                        new_mode = ?ss_guard.turn_mode,
                        player_count = pc,
                        "Turn mode transitioned on reconnecting player join"
                    );

                    // Initialize barrier if transitioning to structured mode
                    if ss_guard.turn_mode.should_use_barrier()
                        && ss_guard.turn_barrier.is_none()
                    {
                        let mp_session =
                            sidequest_game::multiplayer::MultiplayerSession::with_player_ids(
                                ss_guard.players.keys().cloned(),
                            );
                        let adaptive =
                            sidequest_game::barrier::AdaptiveTimeout::default();
                        ss_guard.turn_barrier =
                            Some(sidequest_game::barrier::TurnBarrier::with_adaptive(
                                mp_session, adaptive,
                            ));
                        tracing::info!(
                            player_count = pc,
                            "Initialized turn barrier for reconnecting player"
                        );
                    }

                    // Broadcast targeted PARTY_STATUS to all session members
                    let members: Vec<PartyMember> = ss_guard
                        .players
                        .iter()
                        .map(|(pid, ps)| PartyMember {
                            player_id: pid.clone(),
                            name: ps.player_name.clone(),
                            character_name: ps
                                .character_name
                                .clone()
                                .unwrap_or_else(|| ps.player_name.clone()),
                            current_hp: ps.character_hp,
                            max_hp: ps.character_max_hp,
                            statuses: vec![],
                            class: ps.character_class.clone(),
                            level: ps.character_level,
                            portrait_url: None,
                        })
                        .collect();
                    if !members.is_empty() {
                        let pids: Vec<String> =
                            ss_guard.players.keys().cloned().collect();
                        for target_pid in &pids {
                            let party_msg = GameMessage::PartyStatus {
                                payload: PartyStatusPayload {
                                    members: members.clone(),
                                },
                                player_id: target_pid.clone(),
                            };
                            ss_guard
                                .send_to_player(party_msg, target_pid.clone());
                        }
                    }

                    tracing::info!(
                        player_id = %player_id,
                        player_count = pc,
                        "Reconnecting player joined shared session"
                    );

                    // Send full multiplayer PARTY_STATUS directly to the reconnecting
                    // player (via direct tx, not session channel which may not be subscribed).
                    let all_members: Vec<PartyMember> = ss_guard
                        .players
                        .iter()
                        .map(|(pid, ps)| PartyMember {
                            player_id: pid.clone(),
                            name: ps.player_name.clone(),
                            character_name: ps
                                .character_name
                                .clone()
                                .unwrap_or_else(|| ps.player_name.clone()),
                            current_hp: ps.character_hp,
                            max_hp: ps.character_max_hp,
                            statuses: vec![],
                            class: ps.character_class.clone(),
                            level: ps.character_level,
                            portrait_url: None,
                        })
                        .collect();
                    let member_count = all_members.len();
                    responses.push(GameMessage::PartyStatus {
                        payload: PartyStatusPayload { members: all_members },
                        player_id: player_id.to_string(),
                    });
                    tracing::info!(
                        player_id = %player_id,
                        member_count,
                        "reconnect.party_status — sent full party via direct tx"
                    );

                    // Send TURN_STATUS "resolved" so the reconnecting player's input
                    // is enabled. If it's someone else's turn, the next action will
                    // send a proper TURN_STATUS "active" via global broadcast.
                    if pc > 1 {
                        responses.push(GameMessage::TurnStatus {
                            payload: TurnStatusPayload {
                                player_name: player_name_store
                                    .clone()
                                    .unwrap_or_else(|| "Player".to_string()),
                                status: "resolved".into(),
                                state_delta: None,
                            },
                            player_id: player_id.to_string(),
                        });
                        tracing::info!(player_id = %player_id, pc, "reconnect.turn_status_resolved — sent via direct tx");
                    } else {
                        tracing::info!(player_id = %player_id, pc, "reconnect.solo — no TURN_STATUS needed (single player)");
                    }
                }
            }
            responses
        }
        GameMessage::CharacterCreation { payload, .. } => {
            if !session.is_creating() {
                return vec![error_response(player_id, "Not in character creation state")];
            }
            dispatch_character_creation(
                payload,
                session,
                builder,
                player_name_store,
                character_json,
                character_name,
                character_hp,
                character_max_hp,
                character_level,
                character_xp,
                current_location,
                inventory,
                combat_state,
                chase_state,
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
                shared_session_holder,
                music_director,
                audio_mixer,
                prerender_scheduler,
                state,
                player_id,
                continuity_corrections,
            )
            .await
        }
        GameMessage::PlayerAction { payload, .. } => {
            if !session.is_playing() {
                let err = if session.is_awaiting_connect() {
                    reconnect_required_response(
                        player_id,
                        "Session not established. Please reconnect.",
                    )
                } else {
                    error_response(
                        player_id,
                        &format!("Cannot process action in {} state", session.state_name()),
                    )
                };
                return vec![err];
            }
            dispatch_player_action(
                &payload.action,
                character_name.as_deref().unwrap_or("Unknown"),
                character_hp,
                character_max_hp,
                character_level,
                character_xp,
                current_location,
                inventory,
                character_json,
                combat_state,
                chase_state,
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
                shared_session_holder,
                music_director,
                audio_mixer,
                prerender_scheduler,
                state,
                player_id,
                session.genre_slug().unwrap_or(""),
                session.world_slug().unwrap_or(""),
                player_name_store.as_deref().unwrap_or("Player"),
                continuity_corrections,
            )
            .await
        }
        // All other valid message types in wrong state
        _ => {
            if session.is_awaiting_connect() {
                vec![reconnect_required_response(
                    player_id,
                    "Session not established. Please reconnect.",
                )]
            } else {
                vec![error_response(
                    player_id,
                    &format!("Unexpected message in {} state", session.state_name()),
                )]
            }
        }
    }
}

/// Handle SESSION_EVENT{connect}.
#[allow(clippy::too_many_arguments)]
async fn dispatch_connect(
    payload: &SessionEventPayload,
    session: &mut Session,
    builder: &mut Option<CharacterBuilder>,
    player_name_store: &mut Option<String>,
    character_json_store: &mut Option<serde_json::Value>,
    character_name_store: &mut Option<String>,
    character_hp: &mut i32,
    character_max_hp: &mut i32,
    character_level: &mut u32,
    character_xp: &mut u32,
    current_location: &mut String,
    discovered_regions: &mut Vec<String>,
    trope_defs: &mut Vec<sidequest_genre::TropeDefinition>,
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    world_context: &mut String,
    axes_config: &mut Option<sidequest_genre::AxesConfig>,
    axis_values: &mut Vec<sidequest_game::axis::AxisValue>,
    visual_style: &mut Option<sidequest_genre::VisualStyle>,
    music_director: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_scheduler: &std::sync::Arc<
        tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>,
    >,
    turn_manager: &mut sidequest_game::TurnManager,
    npc_registry: &mut Vec<NpcRegistryEntry>,
    quest_log: &mut std::collections::HashMap<String, String>,
    lore_store: &mut sidequest_game::LoreStore,
    state: &AppState,
    player_id: &str,
    _continuity_corrections: &mut String,
) -> Vec<GameMessage> {
    let genre = payload.genre.as_deref().unwrap_or("");
    let world = payload.world.as_deref().unwrap_or("");
    let pname = payload.player_name.as_deref().unwrap_or("Player");

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
                        if let GameMessage::SessionEvent {
                            ref mut payload, ..
                        } = connected_msg
                        {
                            payload.has_character = Some(true);
                        }
                        responses.push(connected_msg);

                        // Extract character data from saved snapshot
                        if let Some(character) = saved.snapshot.characters.first() {
                            *character_json_store =
                                Some(serde_json::to_value(character).unwrap_or_default());
                            *character_name_store = Some(character.core.name.as_str().to_string());
                            *character_hp = character.core.hp;
                            *character_max_hp = character.core.max_hp;
                            *character_level = character.core.level;
                            *character_xp = character.core.xp;
                        }
                        // Restore location, regions, turn state, and NPC registry from snapshot
                        *current_location = saved.snapshot.location.clone();
                        *discovered_regions = saved.snapshot.discovered_regions.clone();
                        *turn_manager = saved.snapshot.turn_manager.clone();
                        *npc_registry = saved.snapshot.npc_registry.clone();
                        *axis_values = saved.snapshot.axis_values.clone();
                        *trope_states = saved.snapshot.active_tropes.clone();
                        *quest_log = saved.snapshot.quest_log.clone();
                        tracing::info!(
                            trope_count = trope_states.len(),
                            quest_count = quest_log.len(),
                            "reconnect.state_restored — tropes and quests loaded from save"
                        );

                        // Transition session to Playing
                        let _ = session.complete_character_creation();

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
                                            name: c.core.name.as_str().to_string(),
                                            hp: c.core.hp,
                                            max_hp: c.core.max_hp,
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
                                        })
                                        .collect(),
                                    location: saved.snapshot.location.clone(),
                                    quests: saved.snapshot.quest_log.clone(),
                                    turn_count: saved.snapshot.turn_manager.interaction().saturating_sub(1) as u32,
                                }),
                                css: None,
                            },
                            player_id: player_id.to_string(),
                        };
                        responses.push(ready);

                        // Replay essential state for reconnecting client
                        // CHARACTER_SHEET
                        if let Some(character) = saved.snapshot.characters.first() {
                            responses.push(GameMessage::CharacterSheet {
                                payload: CharacterSheetPayload {
                                    name: character.core.name.as_str().to_string(),
                                    class: character.char_class.as_str().to_string(),
                                    race: character.race.as_str().to_string(),
                                    level: character.core.level as u32,
                                    stats: character
                                        .stats
                                        .iter()
                                        .map(|(k, v)| (k.clone(), *v))
                                        .collect(),
                                    abilities: character.hooks.clone(),
                                    backstory: character.backstory.as_str().to_string(),
                                    personality: character.core.personality.as_str().to_string(),
                                    pronouns: character.pronouns.clone(),
                                    equipment: character.core.inventory.items.iter().map(|i| {
                                        if i.equipped {
                                            format!("{} [equipped]", i.name)
                                        } else {
                                            i.name.as_str().to_string()
                                        }
                                    }).collect(),
                                    portrait_url: None,
                                },
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

                        // Last NARRATION — recap or last narrative log entry
                        let recap_text = saved.recap.clone().or_else(|| {
                            saved
                                .snapshot
                                .narrative_log
                                .last()
                                .map(|e| e.content.clone())
                        });
                        if let Some(text) = recap_text {
                            responses.push(GameMessage::Narration {
                                payload: NarrationPayload {
                                    text,
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

                        // PARTY_STATUS
                        {
                            let members: Vec<PartyMember> = saved
                                .snapshot
                                .characters
                                .iter()
                                .map(|c| PartyMember {
                                    player_id: player_id.to_string(),
                                    name: player_name_store.as_deref().unwrap_or("Player").to_string(),
                                    character_name: c.core.name.as_str().to_string(),
                                    current_hp: c.core.hp,
                                    max_hp: c.core.max_hp,
                                    statuses: c.core.statuses.clone(),
                                    class: c.char_class.as_str().to_string(),
                                    level: c.core.level as u32,
                                    portrait_url: None,
                                })
                                .collect();
                            responses.push(GameMessage::PartyStatus {
                                payload: PartyStatusPayload { members },
                                player_id: player_id.to_string(),
                            });
                        }

                        // Initialize audio subsystems for returning player
                        if let Ok(genre_code) = GenreCode::new(genre) {
                            let loader =
                                GenreLoader::new(vec![state.genre_packs_path().to_path_buf()]);
                            if let Ok(pack) = loader.load(&genre_code) {
                                *visual_style = Some(pack.visual_style.clone());
                                *axes_config = Some(pack.axes.clone());
                                *music_director =
                                    Some(sidequest_game::MusicDirector::new(&pack.audio));
                                *audio_mixer.lock().await = Some(sidequest_game::AudioMixer::new(
                                    sidequest_game::DuckConfig::default(),
                                ));
                                *prerender_scheduler.lock().await =
                                    Some(sidequest_game::PrerenderScheduler::new(
                                        sidequest_game::PrerenderConfig::default(),
                                    ));
                                // Load trope definitions for returning player (same logic as start_character_creation)
                                let mut all_tropes = pack.tropes.clone();
                                if let Some(w) = pack.worlds.get(world) {
                                    all_tropes.extend(w.tropes.clone());
                                }
                                for trope in &mut all_tropes {
                                    if trope.id.is_none() {
                                        let slug = trope.name.as_str().to_lowercase().replace(' ', "-")
                                            .replace(|c: char| !c.is_alphanumeric() && c != '-', "");
                                        trope.id = Some(slug);
                                    }
                                }
                                all_tropes.retain(|t| !t.is_abstract);
                                *trope_defs = all_tropes;
                                tracing::info!(count = trope_defs.len(), genre = %genre, "Loaded trope definitions for returning player");

                                tracing::info!(genre = %genre, "Audio subsystems initialized for returning player");

                                // Seed lore store from genre pack (story 11-4)
                                let lore_count =
                                    sidequest_game::seed_lore_from_genre_pack(lore_store, &pack);
                                tracing::info!(
                                    count = lore_count,
                                    genre = %genre,
                                    "rag.lore_store_seeded"
                                );

                                // Inject name bank context for returning player
                                let cultures = pack
                                    .worlds
                                    .get(world)
                                    .filter(|w| !w.cultures.is_empty())
                                    .map(|w| w.cultures.as_slice())
                                    .unwrap_or(&pack.cultures);
                                let name_bank = build_name_bank_context(cultures);
                                if !name_bank.is_empty() {
                                    world_context.push_str(&name_bank);
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
                        if let Some(scene_msg) = start_character_creation(
                            builder,
                            trope_defs,
                            world_context,
                            visual_style,
                            axes_config,
                            music_director,
                            audio_mixer,
                            prerender_scheduler,
                            lore_store,
                            genre,
                            world,
                            state,
                            player_id,
                        )
                        .await
                        {
                            responses.push(scene_msg);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to load saved session, starting fresh");
                        responses.push(connected_msg);
                        if let Some(scene_msg) = start_character_creation(
                            builder,
                            trope_defs,
                            world_context,
                            visual_style,
                            axes_config,
                            music_director,
                            audio_mixer,
                            prerender_scheduler,
                            lore_store,
                            genre,
                            world,
                            state,
                            player_id,
                        )
                        .await
                        {
                            responses.push(scene_msg);
                        }
                    }
                }
            } else {
                // New player — send connected, then start character creation
                responses.push(connected_msg);
                if let Some(scene_msg) = start_character_creation(
                    builder,
                    trope_defs,
                    world_context,
                    visual_style,
                    axes_config,
                    music_director,
                    audio_mixer,
                    prerender_scheduler,
                    lore_store,
                    genre,
                    world,
                    state,
                    player_id,
                )
                .await
                {
                    responses.push(scene_msg);
                }
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
async fn start_character_creation(
    builder: &mut Option<CharacterBuilder>,
    trope_defs_out: &mut Vec<sidequest_genre::TropeDefinition>,
    world_context_out: &mut String,
    visual_style_out: &mut Option<sidequest_genre::VisualStyle>,
    axes_config_out: &mut Option<sidequest_genre::AxesConfig>,
    music_director_out: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer_lock: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_lock: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>>,
    lore_store: &mut sidequest_game::LoreStore,
    genre: &str,
    world_slug: &str,
    state: &AppState,
    player_id: &str,
) -> Option<GameMessage> {
    let genre_code = match GenreCode::new(genre) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(genre = %genre, error = %e, "Invalid genre code");
            return None;
        }
    };

    let loader = GenreLoader::new(vec![state.genre_packs_path().to_path_buf()]);
    let pack = match loader.load(&genre_code) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(genre = %genre, error = %e, "Failed to load genre pack");
            return None;
        }
    };

    *visual_style_out = Some(pack.visual_style.clone());
    *axes_config_out = Some(pack.axes.clone());

    // Initialize audio subsystems from genre pack
    *music_director_out = Some(sidequest_game::MusicDirector::new(&pack.audio));
    *audio_mixer_lock.lock().await = Some(sidequest_game::AudioMixer::new(
        sidequest_game::DuckConfig::default(),
    ));
    *prerender_lock.lock().await = Some(sidequest_game::PrerenderScheduler::new(
        sidequest_game::PrerenderConfig::default(),
    ));
    tracing::info!(genre = %genre, "Audio subsystems initialized from genre pack");

    // Seed lore store from genre pack (story 11-4)
    let lore_count = sidequest_game::seed_lore_from_genre_pack(lore_store, &pack);
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
            ctx.push_str(&format!(
                "\nHistory: {}",
                history.chars().take(200).collect::<String>()
            ));
        }
        if let Some(ref geography) = world.lore.geography {
            ctx.push_str(&format!(
                "\nGeography: {}",
                geography.chars().take(200).collect::<String>()
            ));
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
    let name_bank = build_name_bank_context(cultures);
    if !name_bank.is_empty() {
        world_context_out.push_str(&name_bank);
    }

    // Filter scenes to those with non-empty choices
    let scenes: Vec<_> = pack
        .char_creation
        .into_iter()
        .filter(|s| !s.choices.is_empty())
        .collect();

    if scenes.is_empty() {
        tracing::warn!(genre = %genre, "No character creation scenes with choices");
        return None;
    }

    let b = match CharacterBuilder::try_new(scenes, &pack.rules) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to create CharacterBuilder");
            return None;
        }
    };

    let scene_msg = b.to_scene_message(player_id);
    *builder = Some(b);
    Some(scene_msg)
}

/// Handle CHARACTER_CREATION messages (client choices).
#[allow(clippy::too_many_arguments)]
async fn dispatch_character_creation(
    payload: &CharacterCreationPayload,
    session: &mut Session,
    builder: &mut Option<CharacterBuilder>,
    player_name_store: &mut Option<String>,
    character_json_store: &mut Option<serde_json::Value>,
    character_name_store: &mut Option<String>,
    character_hp: &mut i32,
    character_max_hp: &mut i32,
    character_level: &mut u32,
    character_xp: &mut u32,
    current_location: &mut String,
    inventory: &mut sidequest_game::Inventory,
    combat_state: &mut sidequest_game::combat::CombatState,
    chase_state: &mut Option<sidequest_game::ChaseState>,
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    trope_defs: &mut Vec<sidequest_genre::TropeDefinition>,
    world_context: &str,
    axes_config: &Option<sidequest_genre::AxesConfig>,
    axis_values: &mut Vec<sidequest_game::axis::AxisValue>,
    visual_style: &Option<sidequest_genre::VisualStyle>,
    npc_registry: &mut Vec<NpcRegistryEntry>,
    quest_log: &mut std::collections::HashMap<String, String>,
    narration_history: &mut Vec<String>,
    discovered_regions: &mut Vec<String>,
    turn_manager: &mut sidequest_game::TurnManager,
    lore_store: &mut sidequest_game::LoreStore,
    shared_session_holder: &Arc<
        tokio::sync::Mutex<Option<Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>,
    >,
    music_director: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_scheduler: &std::sync::Arc<
        tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>,
    >,
    state: &AppState,
    player_id: &str,
    continuity_corrections: &mut String,
) -> Vec<GameMessage> {
    let b = match builder.as_mut() {
        Some(b) => b,
        None => return vec![error_response(player_id, "No character builder active")],
    };

    let phase = payload.phase.as_str();
    tracing::info!(phase = %phase, player_id = %player_id, "Character creation phase");

    match phase {
        "scene" => {
            // Parse choice (1-based string → 0-based index)
            let choice_str = payload.choice.as_deref().unwrap_or("1");
            let index = choice_str.parse::<usize>().unwrap_or(1).saturating_sub(1);

            state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "character_creation".to_string(),
                event_type: WatcherEventType::StateTransition,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert(
                        "phase".to_string(),
                        serde_json::Value::String(phase.to_string()),
                    );
                    f.insert("choice_index".to_string(), serde_json::json!(index));
                    f.insert(
                        "player_id".to_string(),
                        serde_json::Value::String(player_id.to_string()),
                    );
                    f
                },
            });

            if let Err(e) = b.apply_choice(index) {
                return vec![error_response(
                    player_id,
                    &format!("Invalid choice: {:?}", e),
                )];
            }

            // Send the next scene or confirmation
            vec![b.to_scene_message(player_id)]
        }
        "confirmation" => {
            // Build the character
            let pname = player_name_store.as_deref().unwrap_or("Player");
            match b.build(pname) {
                Ok(character) => {
                    let char_json = serde_json::to_value(&character).unwrap_or_default();

                    state.send_watcher_event(WatcherEvent {
                        timestamp: chrono::Utc::now(),
                        component: "character_creation".to_string(),
                        event_type: WatcherEventType::StateTransition,
                        severity: Severity::Info,
                        fields: {
                            let mut f = HashMap::new();
                            f.insert(
                                "event".to_string(),
                                serde_json::Value::String("character_built".to_string()),
                            );
                            f.insert(
                                "name".to_string(),
                                serde_json::Value::String(character.core.name.as_str().to_string()),
                            );
                            f.insert(
                                "class".to_string(),
                                serde_json::Value::String(
                                    character.char_class.as_str().to_string(),
                                ),
                            );
                            f.insert(
                                "race".to_string(),
                                serde_json::Value::String(character.race.as_str().to_string()),
                            );
                            f.insert("hp".to_string(), serde_json::json!(character.core.hp));
                            f
                        },
                    });

                    // Store character data — sync ALL mutable fields from the built character
                    *character_name_store = Some(character.core.name.as_str().to_string());
                    *character_hp = character.core.hp;
                    *character_max_hp = character.core.max_hp;
                    *inventory = character.core.inventory.clone();
                    *character_json_store = Some(char_json.clone());
                    tracing::info!(
                        char_name = %character.core.name,
                        hp = character.core.hp,
                        items = character.core.inventory.items.len(),
                        pronouns = %character.pronouns,
                        "chargen.complete — character built, inventory synced"
                    );

                    // Save to SQLite for reconnection across restarts (keyed by player)
                    let genre = session.genre_slug().unwrap_or("").to_string();
                    let world = session.world_slug().unwrap_or("").to_string();
                    let pname_for_save =
                        player_name_store.as_deref().unwrap_or("Player").to_string();
                    let snapshot = sidequest_game::GameSnapshot {
                        genre_slug: genre.clone(),
                        world_slug: world.clone(),
                        characters: vec![character.clone()],
                        location: "Starting area".to_string(),
                        ..Default::default()
                    };
                    if let Err(e) = state
                        .persistence()
                        .save(&genre, &world, &pname_for_save, &snapshot)
                        .await
                    {
                        tracing::warn!(error = %e, genre = %genre, world = %world, player = %pname_for_save, "Failed to persist initial session");
                    }

                    // Transition session to Playing
                    let _ = session.complete_character_creation();
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
                            character_preview: None,
                            choice: None,
                            character: Some(char_json),
                        },
                        player_id: player_id.to_string(),
                    };

                    let ready = GameMessage::SessionEvent {
                        payload: SessionEventPayload {
                            event: "ready".to_string(),
                            player_name: None,
                            genre: None,
                            world: None,
                            has_character: None,
                            initial_state: None,
                            css: None,
                        },
                        player_id: player_id.to_string(),
                    };

                    // Auto-trigger an introductory narration so the game view isn't empty
                    let intro_messages = dispatch_player_action(
                        "I look around and take in my surroundings.",
                        character.core.name.as_str(),
                        character_hp,
                        character_max_hp,
                        character_level,
                        character_xp,
                        current_location,
                        inventory,
                        character_json_store,
                        combat_state,
                        chase_state,
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
                        shared_session_holder,
                        music_director,
                        audio_mixer,
                        prerender_scheduler,
                        state,
                        player_id,
                        &genre,
                        &world,
                        &pname_for_save,
                        continuity_corrections,
                    )
                    .await;

                    // Emit CHARACTER_SHEET for the UI overlay
                    let char_sheet = GameMessage::CharacterSheet {
                        payload: CharacterSheetPayload {
                            name: character.core.name.as_str().to_string(),
                            class: character.char_class.as_str().to_string(),
                            race: character.race.as_str().to_string(),
                            level: character.core.level as u32,
                            stats: character
                                .stats
                                .iter()
                                .map(|(k, v)| (k.clone(), *v))
                                .collect(),
                            abilities: character.hooks.clone(),
                            backstory: character.backstory.as_str().to_string(),
                            personality: character.core.personality.as_str().to_string(),
                            pronouns: character.pronouns.clone(),
                            equipment: character.core.inventory.items.iter().map(|i| {
                                if i.equipped {
                                    format!("{} [equipped]", i.name)
                                } else {
                                    i.name.as_str().to_string()
                                }
                            }).collect(),
                            portrait_url: None,
                        },
                        player_id: player_id.to_string(),
                    };

                    // Emit the character's backstory as a prose narration so
                    // it appears in the narrative view — not just in the overlay.
                    let backstory_narration = GameMessage::Narration {
                        payload: NarrationPayload {
                            text: character.backstory.as_str().to_string(),
                            state_delta: None,
                            footnotes: vec![],
                        },
                        player_id: player_id.to_string(),
                    };
                    let backstory_end = GameMessage::NarrationEnd {
                        payload: NarrationEndPayload { state_delta: None },
                        player_id: player_id.to_string(),
                    };

                    // Add player to shared session and broadcast PARTY_STATUS
                    {
                        let holder = shared_session_holder.lock().await;
                        if let Some(ref ss_arc) = *holder {
                            let mut ss = ss_arc.lock().await;
                            let ps = shared_session::PlayerState::new(
                                player_name_store
                                    .clone()
                                    .unwrap_or_else(|| "Player".to_string()),
                            );
                            ss.players.insert(player_id.to_string(), ps);
                            // Populate character data on the PlayerState
                            if let Some(p) = ss.players.get_mut(player_id) {
                                p.character_name = Some(character.core.name.as_str().to_string());
                                p.character_hp = character.core.hp;
                                p.character_max_hp = character.core.max_hp;
                                p.character_level = character.core.level as u32;
                                p.character_class = character.char_class.as_str().to_string();
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
                            for target_pid in &existing_pids {
                                ss.send_to_player(
                                    GameMessage::Narration {
                                        payload: NarrationPayload {
                                            text: arrival_text.clone(),
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
                            // Build and send targeted PARTY_STATUS to each session member
                            // Each player gets their own player_id so the client HUD
                            // shows the correct identity.
                            let members: Vec<PartyMember> = ss
                                .players
                                .iter()
                                .map(|(pid, ps)| {
                                    if pid == player_id {
                                        // Current player — use local character data
                                        PartyMember {
                                            player_id: pid.clone(),
                                            name: ps.player_name.clone(),
                                            character_name: character.core.name.as_str().to_string(),
                                            current_hp: character.core.hp,
                                            max_hp: character.core.max_hp,
                                            statuses: character.core.statuses.clone(),
                                            class: character.char_class.as_str().to_string(),
                                            level: character.core.level as u32,
                                            portrait_url: None,
                                        }
                                    } else {
                                        // Other player — use PlayerState fields
                                        PartyMember {
                                            player_id: pid.clone(),
                                            name: ps.player_name.clone(),
                                            character_name: ps.character_name.clone().unwrap_or_else(|| ps.player_name.clone()),
                                            current_hp: ps.character_hp,
                                            max_hp: ps.character_max_hp,
                                            statuses: vec![],
                                            class: ps.character_class.clone(),
                                            level: ps.character_level,
                                            portrait_url: None,
                                        }
                                    }
                                })
                                .collect();
                            if !members.is_empty() {
                                let player_ids: Vec<String> = ss.players.keys().cloned().collect();
                                for target_pid in &player_ids {
                                    let party_msg = GameMessage::PartyStatus {
                                        payload: PartyStatusPayload { members: members.clone() },
                                        player_id: target_pid.clone(),
                                    };
                                    ss.send_to_player(party_msg, target_pid.clone());
                                }
                            }
                            let pc = ss.player_count();
                            tracing::info!(
                                player_id = %player_id,
                                player_count = pc,
                                "Player joined shared session"
                            );
                            state.send_watcher_event(WatcherEvent {
                                timestamp: chrono::Utc::now(),
                                component: "multiplayer".to_string(),
                                event_type: WatcherEventType::StateTransition,
                                severity: Severity::Info,
                                fields: {
                                    let mut f = HashMap::new();
                                    f.insert(
                                        "event".to_string(),
                                        serde_json::json!("session_joined"),
                                    );
                                    f.insert(
                                        "session_key".to_string(),
                                        serde_json::json!(format!("{}:{}", genre, world)),
                                    );
                                    f.insert("player_count".to_string(), serde_json::json!(pc));
                                    f
                                },
                            });

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
                                                description: NonBlankString::new("barrier placeholder").unwrap(),
                                                personality: NonBlankString::new("n/a").unwrap(),
                                                level: 1, hp: 1, max_hp: 1, ac: 10, xp: 0,
                                                statuses: vec![],
                                                inventory: Inventory::default(),
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
                                        }
                                    };
                                    let _ = barrier.add_player(player_id.to_string(), placeholder_char);
                                    tracing::info!(player_id = %player_id, "Added player to existing barrier");
                                } else {
                                    let mp_session = sidequest_game::multiplayer::MultiplayerSession::with_player_ids(
                                        ss.players.keys().cloned(),
                                    );
                                    let adaptive = sidequest_game::barrier::AdaptiveTimeout::default();
                                    ss.turn_barrier = Some(sidequest_game::barrier::TurnBarrier::with_adaptive(
                                        mp_session, adaptive,
                                    ));
                                    tracing::info!(player_count = pc, "Initialized turn barrier for multiplayer");
                                }
                            }
                        }
                    }

                    let mut msgs = vec![
                        complete,
                        char_sheet,
                        backstory_narration,
                        backstory_end,
                        ready,
                    ];
                    msgs.extend(intro_messages);
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

/// Handle PLAYER_ACTION — send THINKING, narration, NARRATION_END, PARTY_STATUS.
#[allow(clippy::too_many_arguments)]
async fn dispatch_player_action(
    action: &str,
    char_name: &str,
    hp: &mut i32,
    max_hp: &mut i32,
    level: &mut u32,
    xp: &mut u32,
    current_location: &mut String,
    inventory: &mut sidequest_game::Inventory,
    character_json: &mut Option<serde_json::Value>,
    combat_state: &mut sidequest_game::combat::CombatState,
    chase_state: &mut Option<sidequest_game::ChaseState>,
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    trope_defs: &[sidequest_genre::TropeDefinition],
    world_context: &str,
    axes_config: &Option<sidequest_genre::AxesConfig>,
    axis_values: &mut Vec<sidequest_game::axis::AxisValue>,
    visual_style: &Option<sidequest_genre::VisualStyle>,
    npc_registry: &mut Vec<NpcRegistryEntry>,
    quest_log: &mut std::collections::HashMap<String, String>,
    narration_history: &mut Vec<String>,
    discovered_regions: &mut Vec<String>,
    turn_manager: &mut sidequest_game::TurnManager,
    lore_store: &sidequest_game::LoreStore,
    shared_session_holder: &Arc<
        tokio::sync::Mutex<Option<Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>,
    >,
    music_director: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_scheduler: &std::sync::Arc<
        tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>,
    >,
    state: &AppState,
    player_id: &str,
    genre_slug: &str,
    world_slug: &str,
    player_name_for_save: &str,
    continuity_corrections: &mut String,
) -> Vec<GameMessage> {
    // Sync world-level state from shared session (if multiplayer)
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            ss.sync_to_locals(
                current_location,
                npc_registry,
                narration_history,
                discovered_regions,
                trope_states,
            );
            // Sync per-player state from barrier modifications (HP, inventory, combat, etc.)
            ss.sync_player_to_locals(
                player_id,
                hp,
                max_hp,
                level,
                xp,
                inventory,
                combat_state,
                chase_state,
                character_json,
            );
            let pc = ss.player_count();
            if pc > 1 {
                state.send_watcher_event(WatcherEvent {
                    timestamp: chrono::Utc::now(),
                    component: "multiplayer".to_string(),
                    event_type: WatcherEventType::AgentSpanOpen,
                    severity: Severity::Info,
                    fields: {
                        let mut f = HashMap::new();
                        f.insert("event".to_string(), serde_json::json!("multiplayer_action"));
                        f.insert(
                            "session_key".to_string(),
                            serde_json::json!(format!("{}:{}", genre_slug, world_slug)),
                        );
                        f.insert("player_id".to_string(), serde_json::json!(player_id));
                        f.insert("party_size".to_string(), serde_json::json!(pc));
                        f
                    },
                });
            }
        }
    }

    // Watcher: action received
    let turn_number = turn_manager.interaction();
    state.send_watcher_event(WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "game".to_string(),
        event_type: WatcherEventType::AgentSpanOpen,
        severity: Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert(
                "action".to_string(),
                serde_json::Value::String(action.to_string()),
            );
            f.insert(
                "player".to_string(),
                serde_json::Value::String(char_name.to_string()),
            );
            f.insert("turn_number".to_string(), serde_json::json!(turn_number));
            f
        },
    });

    // TURN_STATUS "active" — tell all players whose turn it is BEFORE the LLM call.
    // Sent via GLOBAL broadcast (not session channel) because the session channel
    // subscriber may not be initialized yet — broadcast::channel drops messages
    // sent before subscription. Global broadcast reaches all connected clients.
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            if ss.players.len() > 1 {
                let turn_active = GameMessage::TurnStatus {
                    payload: TurnStatusPayload {
                        player_name: player_name_for_save.to_string(),
                        status: "active".into(),
                        state_delta: None,
                    },
                    player_id: player_id.to_string(),
                };
                let _ = state.broadcast(turn_active);
                tracing::info!(player_id = %player_id, player_name = %player_name_for_save, "turn_status.active broadcast to all clients");
            }
        }
    }

    // THINKING indicator — send eagerly BEFORE LLM call so UI shows it immediately.
    // Send only to the acting player via session channel (not global broadcast)
    // so that other players' input is not blocked by the "narrator thinking" lock.
    let thinking = GameMessage::Thinking {
        payload: ThinkingPayload {},
        player_id: player_id.to_string(),
    };
    tracing::info!(player_id = %player_id, "thinking.sent");
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            ss.send_to_player(thinking.clone(), player_id.to_string());
        } else {
            // Single-player fallback: use global broadcast
            let _ = state.broadcast(thinking.clone());
        }
    }

    // Slash command interception — route /commands to mechanical handlers, not the LLM.
    if action.starts_with('/') {
        use sidequest_game::commands::{
            GmCommand, InventoryCommand, MapCommand, QuestsCommand, SaveCommand, StatusCommand,
        };
        use sidequest_game::slash_router::SlashRouter;
        use sidequest_game::state::GameSnapshot;

        let mut router = SlashRouter::new();
        router.register(Box::new(StatusCommand));
        router.register(Box::new(InventoryCommand));
        router.register(Box::new(MapCommand));
        router.register(Box::new(QuestsCommand));
        router.register(Box::new(SaveCommand));
        router.register(Box::new(GmCommand));
        if let Some(ref ac) = axes_config {
            router.register(Box::new(sidequest_game::ToneCommand::new(ac.clone())));
        }

        // Build a minimal GameSnapshot from the local session state.
        let snapshot = {
            let mut snap = GameSnapshot {
                genre_slug: genre_slug.to_string(),
                world_slug: world_slug.to_string(),
                location: current_location.clone(),
                combat: combat_state.clone(),
                chase: chase_state.clone(),
                axis_values: axis_values.clone(),
                active_tropes: trope_states.clone(),
                quest_log: quest_log.clone(),
                ..GameSnapshot::default()
            };
            // Reconstruct a minimal Character from loose variables.
            if let Some(ref cj) = character_json {
                if let Ok(mut ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone())
                {
                    // Sync mutable fields that may have diverged from the JSON snapshot.
                    ch.core.hp = *hp;
                    ch.core.max_hp = *max_hp;
                    ch.core.level = *level;
                    ch.core.inventory = inventory.clone();
                    snap.characters.push(ch);
                }
            }
            snap
        };

        if let Some(cmd_result) = router.try_dispatch(action, &snapshot) {
            tracing::info!(command = %action, result_type = ?std::mem::discriminant(&cmd_result), "slash_command.dispatched");
            let text = match &cmd_result {
                sidequest_game::slash_router::CommandResult::Display(t) => t.clone(),
                sidequest_game::slash_router::CommandResult::Error(e) => e.clone(),
                sidequest_game::slash_router::CommandResult::StateMutation(patch) => {
                    // Apply location/region changes from /gm commands.
                    if let Some(ref loc) = patch.location {
                        *current_location = loc.clone();
                    }
                    if let Some(ref hp_changes) = patch.hp_changes {
                        for (_target, delta) in hp_changes {
                            *hp = (*hp + delta).max(0);
                        }
                    }
                    format!("GM command applied.")
                }
                sidequest_game::slash_router::CommandResult::ToneChange(new_values) => {
                    *axis_values = new_values.clone();
                    format!("Tone updated.")
                }
                _ => "Command executed.".to_string(),
            };

            // Watcher: slash command handled
            state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "game".to_string(),
                event_type: WatcherEventType::AgentSpanClose,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert(
                        "slash_command".to_string(),
                        serde_json::Value::String(action.to_string()),
                    );
                    f.insert("result_len".to_string(), serde_json::json!(text.len()));
                    f
                },
            });

            return vec![
                GameMessage::Narration {
                    payload: NarrationPayload {
                        text,
                        state_delta: None,
                        footnotes: vec![],
                    },
                    player_id: player_id.to_string(),
                },
                GameMessage::NarrationEnd {
                    payload: NarrationEndPayload { state_delta: None },
                    player_id: player_id.to_string(),
                },
            ];
        }
    }

    // Seed starter tropes if none are active yet (first turn)
    if trope_states.is_empty() && !trope_defs.is_empty() {
        // Prefer tropes with passive_progression so tick() can advance them.
        // Fall back to any trope if none have passive_progression.
        let mut seedable: Vec<&sidequest_genre::TropeDefinition> = trope_defs
            .iter()
            .filter(|d| d.passive_progression.is_some() && d.id.is_some())
            .collect();
        if seedable.is_empty() {
            seedable = trope_defs.iter().filter(|d| d.id.is_some()).collect();
        }
        let seed_count = seedable.len().min(3);
        tracing::info!(
            total_defs = trope_defs.len(),
            with_progression = trope_defs
                .iter()
                .filter(|d| d.passive_progression.is_some())
                .count(),
            seedable = seedable.len(),
            seed_count = seed_count,
            "Trope seeding — selecting starter tropes"
        );
        for def in &seedable[..seed_count] {
            if let Some(id) = &def.id {
                sidequest_game::trope::TropeEngine::activate(trope_states, id);
                tracing::info!(
                    trope_id = %id,
                    name = %def.name,
                    has_progression = def.passive_progression.is_some(),
                    "Seeded starter trope"
                );
                state.send_watcher_event(WatcherEvent {
                    timestamp: chrono::Utc::now(),
                    component: "trope".to_string(),
                    event_type: WatcherEventType::StateTransition,
                    severity: Severity::Info,
                    fields: {
                        let mut f = HashMap::new();
                        f.insert(
                            "event".to_string(),
                            serde_json::Value::String("trope_activated".to_string()),
                        );
                        f.insert(
                            "trope_id".to_string(),
                            serde_json::Value::String(id.clone()),
                        );
                        f
                    },
                });
            }
        }
    }

    // Build active trope context for the narrator prompt
    let trope_context = if trope_states.is_empty() {
        String::new()
    } else {
        let mut lines = vec!["Active narrative arcs:".to_string()];
        for ts in trope_states.iter() {
            if let Some(def) = trope_defs
                .iter()
                .find(|d| d.id.as_deref() == Some(ts.trope_definition_id()))
            {
                lines.push(format!(
                    "- {} ({}% progressed): {}",
                    def.name,
                    (ts.progression() * 100.0) as u32,
                    def.description
                        .as_deref()
                        .unwrap_or("")
                        .chars()
                        .take(120)
                        .collect::<String>(),
                ));
                // Include the next unfired escalation beat as a hint
                for beat in &def.escalation {
                    if beat.at > ts.progression() {
                        lines.push(format!(
                            "  → Next beat at {}%: {}",
                            (beat.at * 100.0) as u32,
                            beat.event.chars().take(80).collect::<String>()
                        ));
                        break;
                    }
                }
            }
        }
        lines.join("\n")
    };

    // Build state summary for grounding narration (Bug 1: include location + entities)
    let mut state_summary = format!(
        "Character: {} (HP {}/{}, Level {}, XP {})\nGenre: {}",
        char_name, *hp, *max_hp, *level, *xp, genre_slug,
    );

    // Inject party roster so the narrator knows which characters are player-controlled
    // and never puppets them (gives them dialogue, actions, or internal state).
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            let other_pcs: Vec<String> = ss
                .players
                .iter()
                .filter(|(pid, _)| pid.as_str() != player_id)
                .filter_map(|(_, ps)| ps.character_name.clone())
                .collect();
            let co_located_names: Vec<String> = ss
                .co_located_players(player_id)
                .iter()
                .filter_map(|pid| ss.players.get(pid.as_str()).and_then(|ps| ps.character_name.clone()))
                .collect();

            if !other_pcs.is_empty() {
                state_summary.push_str("\n\nPLAYER-CONTROLLED CHARACTERS IN THE PARTY:\n");
                state_summary.push_str("The following characters are controlled by OTHER human players:\n");
                for name in &other_pcs {
                    state_summary.push_str(&format!("- {}\n", name));
                }
                if !co_located_names.is_empty() {
                    state_summary.push_str(&format!(
                        "\nCO-LOCATION — HARD RULE: The following party members are RIGHT HERE with the acting player: {}. \
                         They are physically present at the SAME location. The narrator MUST acknowledge their presence \
                         in the scene. Do NOT narrate them as being elsewhere or arriving from somewhere else. \
                         They are already here.\n",
                        co_located_names.join(", ")
                    ));
                }
                state_summary.push_str(concat!(
                    "\n\nPLAYER AGENCY — ABSOLUTE RULE (violations break the game):\n",
                    "You MUST NOT write dialogue, actions, thoughts, feelings, gestures, or internal ",
                    "state for ANY player character — including the acting player beyond their stated action.\n",
                    "FORBIDDEN examples:\n",
                    "- \"Laverne holds up their power glove. 'I've got the strong hand covered.'\" (writing dialogue FOR a player)\n",
                    "- \"Shirley nudges Laverne with an elbow\" (scripting PC-to-PC physical interaction)\n",
                    "- \"Kael's heart races as he...\" (writing internal state for a player)\n",
                    "ALLOWED examples:\n",
                    "- \"Laverne is nearby, power glove faintly humming.\" (describing presence without action)\n",
                    "- \"The other party members are within earshot.\" (acknowledging presence)\n",
                    "Players control their OWN characters. You control the WORLD, NPCs, and narration only.",
                ));
                state_summary.push_str(
                    "\n\nPERSPECTIVE MODE: Third-person omniscient. \
                     You are narrating for multiple players simultaneously. \
                     Do NOT use 'you' for any character — including the acting player. \
                     All characters are named explicitly in third-person. \
                     Correct: 'Mira surveys the gantry. Kael moves to cover.' \
                     Wrong: 'You survey the gantry.'"
                );
            }
        }
    }

    // Location constraint — prevent narrator from teleporting between scenes
    if !current_location.is_empty() {
        // Dialogue context: if the player interacted with an NPC in the last 2 turns,
        // any location mention in the action is likely dialogue (describing a place to
        // the NPC), not a travel intent. Strengthen the stay-put constraint.
        let turn_approx = turn_manager.interaction() as u32;
        let recent_npc_interaction = npc_registry
            .iter()
            .any(|e| turn_approx.saturating_sub(e.last_seen_turn) <= 2);
        let extra_dialogue_guard = if recent_npc_interaction {
            " IMPORTANT: The player is currently in dialogue with an NPC. If the player's \
             action mentions a location or place name, they are TALKING ABOUT that place, \
             NOT traveling there. Keep the scene at the current location. Only move if the \
             player explicitly ends the conversation and states they are leaving."
        } else {
            ""
        };
        state_summary.push_str(&format!(
            "\n\nLOCATION CONSTRAINT — THIS IS A HARD RULE:\nThe player is at: {}\nYou MUST continue the scene at this location. Do NOT introduce a new setting, move to a different area, or describe the player arriving somewhere else UNLESS the player explicitly says they want to travel or leave. If the player's action implies staying here, describe what happens HERE. Only change location when the player takes a deliberate travel action (e.g., 'I go to...', 'I leave...', 'I head north').{}",
            current_location, extra_dialogue_guard
        ));
    }

    // Inventory constraint — the narrator must respect the character sheet
    let equipped_count = inventory.items.iter().filter(|i| i.equipped).count();
    tracing::debug!(
        items = inventory.items.len(),
        equipped = equipped_count,
        gold = inventory.gold,
        "narrator_prompt.inventory_constraint — injecting character sheet"
    );
    state_summary.push_str("\n\nCHARACTER SHEET — INVENTORY (canonical, overrides narration):");
    if !inventory.items.is_empty() {
        state_summary.push_str("\nThe player currently possesses EXACTLY these items:");
        for item in &inventory.items {
            let equipped_tag = if item.equipped { " [EQUIPPED]" } else { "" };
            let qty_tag = if item.quantity > 1 {
                format!(" (x{})", item.quantity)
            } else {
                String::new()
            };
            state_summary.push_str(&format!(
                "\n- {}{}{} — {} ({})",
                item.name, equipped_tag, qty_tag, item.description, item.category
            ));
        }
        state_summary.push_str(&format!("\nGold: {}", inventory.gold));
        state_summary.push_str(concat!(
            "\n\nINVENTORY RULES (HARD CONSTRAINTS — violations break the game):",
            "\n1. If the player uses an item on this list, it WORKS. The item is real and present.",
            "\n2. If the player uses an item NOT on this list, it FAILS — they don't have it.",
            "\n3. NEVER narrate an item being lost, stolen, broken, or missing unless the game",
            "\n   engine explicitly removes it. The inventory list above is the TRUTH.",
            "\n4. [EQUIPPED] items are currently in hand/worn — the player does not need to 'find'",
            "\n   or 'reach for' them. They are ready to use immediately.",
        ));
    } else {
        state_summary.push_str("\nThe player has NO items. If the player claims to use any item, the narrator MUST reject it — they have nothing in their possession yet.");
    }

    // Quest log — inject active quests so narrator can reference them
    if !quest_log.is_empty() {
        state_summary.push_str("\n\nACTIVE QUESTS:\n");
        for (quest_name, status) in quest_log.iter() {
            state_summary.push_str(&format!("- {}: {}\n", quest_name, status));
        }
        state_summary.push_str("Reference active quests when narratively relevant. Update quest status in quest_updates when objectives change.\n");
    }

    // Bug 6: Include chase state if active
    if let Some(ref cs) = chase_state {
        state_summary.push_str(&format!(
            "\nACTIVE CHASE: {:?} (round {}, separation {})",
            cs.chase_type(),
            cs.round(),
            cs.separation()
        ));
    }

    // Include character abilities and mutations so the narrator knows what
    // the character can and cannot do (prevents hallucinated abilities).
    if let Some(ref cj) = character_json {
        // Extract hooks (narrative abilities, mutations, etc.)
        if let Some(hooks) = cj.get("hooks").and_then(|h| h.as_array()) {
            let hook_strs: Vec<&str> = hooks.iter().filter_map(|v| v.as_str()).collect();
            if !hook_strs.is_empty() {
                state_summary.push_str("\n\nABILITY CONSTRAINTS — THIS IS A HARD RULE:\n");
                state_summary.push_str("The character can ONLY use the following abilities. Any action that requires a power, mutation, or supernatural ability NOT on this list MUST fail or be reinterpreted as a mundane attempt. Do NOT grant the character abilities they do not have.\n");
                state_summary.push_str("Allowed abilities:\n");
                for h in &hook_strs {
                    state_summary.push_str(&format!("- {}\n", h));
                }
                state_summary.push_str("If the player attempts to use an ability NOT listed above, describe the attempt failing or reframe it as a non-supernatural action.\n");
                state_summary.push_str("PROACTIVE MUTATION NARRATION: When the scene naturally creates an opportunity for the character's abilities/mutations to be relevant (sensory input, danger, social situations), weave them into the narration subtly. A psychic character might catch stray thoughts; a bioluminescent character's skin might flicker in darkness. Don't force it every turn, but don't ignore mutations either — they define who the character IS.\n");
            }
        }
        // Extract backstory
        if let Some(backstory) = cj.get("backstory").and_then(|b| b.as_str()) {
            state_summary.push_str(&format!("\nBackstory: {}", backstory));
        }
        // Extract class and race for narrator awareness
        if let Some(class) = cj.get("char_class").and_then(|c| c.as_str()) {
            state_summary.push_str(&format!("\nClass: {}", class));
        }
        if let Some(race) = cj.get("race").and_then(|r| r.as_str()) {
            state_summary.push_str(&format!("\nRace/Origin: {}", race));
        }
        if let Some(pronouns) = cj.get("pronouns").and_then(|p| p.as_str()) {
            if !pronouns.is_empty() {
                state_summary.push_str(&format!("\nPronouns: {} — ALWAYS use these pronouns for this character.", pronouns));
                tracing::debug!(pronouns = %pronouns, "narrator_prompt.pronouns — injected into state_summary");
            }
        }
    }

    if !world_context.is_empty() {
        state_summary.push('\n');
        state_summary.push_str(world_context);
    }

    // Inject known locations so the narrator uses canonical place names
    if !discovered_regions.is_empty() {
        state_summary.push_str("\n\nKNOWN LOCATIONS IN THIS WORLD:\n");
        state_summary.push_str("Use ONLY these location names when referring to places the party has visited or heard about. Do NOT invent new settlement names.\n");
        for region in discovered_regions.iter() {
            state_summary.push_str(&format!("- {}\n", region));
        }
    }
    // Also inject cartography region names from the shared session (if available)
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            if !ss.region_names.is_empty() {
                if discovered_regions.is_empty() {
                    state_summary.push_str("\n\nWORLD LOCATIONS (from cartography):\n");
                    state_summary.push_str("Use these canonical location names. Do NOT invent new ones.\n");
                } else {
                    state_summary.push_str("Additional world locations (not yet visited):\n");
                }
                for (region_id, _display_name) in &ss.region_names {
                    if !discovered_regions.iter().any(|r| r.to_lowercase() == *region_id) {
                        state_summary.push_str(&format!("- {}\n", region_id));
                    }
                }
            }
        }
    }

    if !trope_context.is_empty() {
        state_summary.push('\n');
        state_summary.push_str(&trope_context);
    }

    // Inject tone context from narrative axes (story F2/F10)
    if let Some(ref ac) = axes_config {
        let tone_text = sidequest_game::format_tone_context(ac, axis_values);
        if !tone_text.is_empty() {
            state_summary.push_str(&tone_text);
        }
    }

    // Bug 17: Include recent narration history so the narrator maintains continuity
    if !narration_history.is_empty() {
        state_summary.push_str("\n\nRECENT CONVERSATION HISTORY (multiple players, most recent last):\nEntries are tagged with [CharacterName]. Only narrate for the ACTING player — do not continue another player's scene:\n");
        // Include at most the last 10 turns to stay within context limits
        let start = narration_history.len().saturating_sub(10);
        for entry in &narration_history[start..] {
            state_summary.push_str(entry);
            state_summary.push('\n');
        }
    }

    // Inject NPC registry so the narrator maintains identity consistency
    let npc_context = build_npc_registry_context(npc_registry);
    if !npc_context.is_empty() {
        state_summary.push_str(&npc_context);
    }

    // Inject lore context from genre pack — budget-aware selection (story 11-4)
    {
        let context_hint = if !current_location.is_empty() {
            Some(current_location.as_str())
        } else {
            None
        };
        let lore_budget = 500; // ~500 tokens for lore context
        let selected =
            sidequest_game::select_lore_for_prompt(lore_store, lore_budget, context_hint);
        if !selected.is_empty() {
            let lore_text = sidequest_game::format_lore_context(&selected);
            tracing::info!(
                fragments = selected.len(),
                tokens = selected.iter().map(|f| f.token_estimate()).sum::<usize>(),
                hint = ?context_hint,
                "rag.lore_injected_to_prompt"
            );
            state_summary.push_str("\n\n");
            state_summary.push_str(&lore_text);
        }
    }

    // F9: Wish Consequence Engine — detect power-grab actions and inject consequence context
    {
        let mut engine = sidequest_game::WishConsequenceEngine::new();
        if let Some(wish) = engine.evaluate(char_name, action) {
            let wish_context = sidequest_game::WishConsequenceEngine::build_prompt_context(&wish);
            tracing::info!(
                wisher = %wish.wisher_name,
                category = ?wish.consequence_category,
                "wish_consequence.power_grab_detected"
            );
            state_summary.push_str(&wish_context);
        }
    }

    // Inject continuity corrections from the previous turn (if any)
    if !continuity_corrections.is_empty() {
        state_summary.push_str("\n\n");
        state_summary.push_str(continuity_corrections);
        tracing::info!(
            corrections_len = continuity_corrections.len(),
            "continuity.corrections_injected_to_prompt"
        );
        // Clear after injection — corrections are one-shot
        continuity_corrections.clear();
    }

    // Check if barrier mode is active (Structured/Cinematic turn mode).
    // If active, submit action to barrier, send "waiting" to this player via session
    // channel, then await barrier resolution inline. After resolution, override the
    // action with the combined party context and fall through to the normal pipeline.
    // This ensures ALL post-narration systems (HP, combat, tropes, quests, inventory,
    // persistence, music, render, etc.) run for barrier turns — not just narration.
    // Track barrier claim state for Bug 2 fix: only the claiming handler runs narrator.
    let mut barrier_claimed: Option<bool> = None;
    let mut barrier_for_narration: Option<sidequest_game::barrier::TurnBarrier> = None;
    let barrier_combined_action: Option<String> = {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            tracing::debug!(
                turn_mode = ?ss.turn_mode,
                player_count = ss.players.len(),
                has_barrier = ss.turn_barrier.is_some(),
                "turn_mode.check — evaluating barrier vs freeplay"
            );
            if ss.turn_mode.should_use_barrier() {
                if let Some(ref barrier) = ss.turn_barrier {
                    // Submit action to barrier (doesn't trigger narration yet)
                    tracing::info!(player_id = %player_id, "barrier.submit — action submitted, waiting for other players");
                    barrier.submit_action(player_id, action);

                    // Broadcast TURN_STATUS "active" so other players' UIs know this player submitted
                    let turn_submitted = GameMessage::TurnStatus {
                        payload: TurnStatusPayload {
                            player_name: player_name_for_save.to_string(),
                            status: "active".into(),
                            state_delta: None,
                        },
                        player_id: player_id.to_string(),
                    };
                    let _ = state.broadcast(turn_submitted);
                    tracing::info!(player_name = %player_name_for_save, "barrier.turn_status.active — broadcast submission notification");
                    let barrier_clone = barrier.clone();

                    // Send "waiting" to this player via session channel (writer task delivers it)
                    ss.send_to_player(
                        GameMessage::SessionEvent {
                            payload: SessionEventPayload {
                                event: "waiting".to_string(),
                                player_name: None,
                                genre: None,
                                world: None,
                                has_character: None,
                                initial_state: None,
                                css: None,
                            },
                            player_id: player_id.to_string(),
                        },
                        player_id.to_string(),
                    );

                    drop(ss);
                    drop(holder);

                    // Await barrier resolution inline — player is waiting, handler blocked is fine
                    let barrier_result = barrier_clone.wait_for_turn().await;
                    tracing::info!(
                        timed_out = barrier_result.timed_out,
                        claimed = barrier_result.claimed_resolution,
                        missing = ?barrier_result.missing_players,
                        genre = %genre_slug,
                        world = %world_slug,
                        "Turn barrier resolved"
                    );

                    // Bug 2 fix: Track claim state so only the claiming handler calls narrator.
                    barrier_claimed = Some(barrier_result.claimed_resolution);
                    barrier_for_narration = Some(barrier_clone.clone());

                    // Bug 1 fix: Read actions from the barrier's internal session,
                    // NOT from SharedGameSession.multiplayer (which is a separate empty session).
                    let named_actions = barrier_clone.named_actions();
                    let combined = named_actions
                        .iter()
                        .map(|(name, act)| format!("{}: {}", name, act))
                        .collect::<Vec<_>>()
                        .join("\n");

                    // Prepend combined actions + perspective instruction to state_summary
                    state_summary = format!(
                        "Combined party actions:\n{}\n\nPERSPECTIVE: Write in third-person omniscient. Do NOT use 'you' for any character. Name all characters explicitly.\n\n{}",
                        combined, state_summary
                    );

                    Some(combined)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    };

    // Use combined action for barrier turns, original action for FreePlay
    let effective_action: std::borrow::Cow<str> = match &barrier_combined_action {
        Some(combined) => std::borrow::Cow::Borrowed(combined.as_str()),
        None => std::borrow::Cow::Borrowed(action),
    };

    // Preprocess raw player input — STT cleanup + three-perspective rewrite.
    // Uses haiku-tier LLM with 15s timeout; falls back to mechanical rewrite on failure.
    let preprocessed = sidequest_agents::preprocessor::preprocess_action(&effective_action, char_name);
    tracing::info!(
        raw = %action,
        you = %preprocessed.you,
        named = %preprocessed.named,
        intent = %preprocessed.intent,
        "Action preprocessed"
    );

    // Process the action through GameService.
    // Bug 2 fix: For barrier turns, only the claiming handler calls the narrator.
    // Non-claiming handlers wait for the narration via the barrier's async channel.
    let result = if barrier_claimed == Some(false) {
        // Non-claiming handler — wait for the claiming handler to finish narration
        let narration = barrier_for_narration
            .as_ref()
            .expect("barrier_for_narration must be set when barrier_claimed is Some")
            .wait_for_resolution_narration()
            .await;
        tracing::info!(
            narration_len = narration.len(),
            "barrier.non_claimer — received narration from claiming handler"
        );
        sidequest_agents::orchestrator::ActionResult {
            narration,
            state_delta: None,
            combat_events: vec![],
            combat_patch: None,
            is_degraded: false,
            classified_intent: None,
            agent_name: None,
            footnotes: vec![],
            items_gained: vec![],
            npcs_present: vec![],
            quest_updates: HashMap::new(),
        }
    } else {
        // Claiming handler (barrier) or FreePlay — run narrator normally
        let context = TurnContext {
            state_summary: Some(state_summary),
            in_combat: combat_state.in_combat(),
            in_chase: chase_state.is_some(),
        };
        let narrator_result = state
            .game_service()
            .process_action(&preprocessed.you, &context);

        // If this is the claiming barrier handler, store narration for non-claimers
        if barrier_claimed == Some(true) {
            if let Some(ref barrier) = barrier_for_narration {
                barrier.store_resolution_narration(narrator_result.narration.clone());
                tracing::info!(
                    narration_len = narrator_result.narration.len(),
                    "barrier.claimer — stored narration for non-claiming handlers"
                );
            }
        }

        narrator_result
    };

    // Watcher: narration generated (with intent classification and agent routing)
    state.send_watcher_event(WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "game".to_string(),
        event_type: WatcherEventType::AgentSpanClose,
        severity: Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert(
                "narration_len".to_string(),
                serde_json::json!(result.narration.len()),
            );
            f.insert(
                "is_degraded".to_string(),
                serde_json::json!(result.is_degraded),
            );
            f.insert("turn_number".to_string(), serde_json::json!(turn_number));
            if let Some(ref intent) = result.classified_intent {
                f.insert("classified_intent".to_string(), serde_json::json!(intent));
            }
            if let Some(ref agent) = result.agent_name {
                f.insert("agent_routed_to".to_string(), serde_json::json!(agent));
            }
            f
        },
    });

    let mut messages = vec![];

    // Extract location header from narration (format: **Location Name**\n\n...)
    // Bug 1: Update current_location so subsequent turns maintain continuity
    let narration_text = &result.narration;
    if let Some(location) = extract_location_header(narration_text) {
        let is_new = !discovered_regions.iter().any(|r| r == &location);
        *current_location = location.clone();
        if is_new {
            discovered_regions.push(location.clone());
        }
        tracing::info!(
            location = %location,
            is_new,
            total_discovered = discovered_regions.len(),
            "location.changed"
        );
        state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "game".to_string(),
            event_type: WatcherEventType::StateTransition,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert(
                    "event".to_string(),
                    serde_json::Value::String("location_changed".to_string()),
                );
                f.insert(
                    "location".to_string(),
                    serde_json::Value::String(location.clone()),
                );
                f.insert("turn_number".to_string(), serde_json::json!(turn_number));
                f
            },
        });
        messages.push(GameMessage::ChapterMarker {
            payload: ChapterMarkerPayload {
                title: Some(location.clone()),
                location: Some(location.clone()),
            },
            player_id: player_id.to_string(),
        });
        // Build explored locations from discovered_regions
        let explored_locs: Vec<sidequest_protocol::ExploredLocation> = discovered_regions
            .iter()
            .map(|name| sidequest_protocol::ExploredLocation {
                name: name.clone(),
                x: 0,
                y: 0,
                location_type: String::new(),
                connections: vec![],
            })
            .collect();
        messages.push(GameMessage::MapUpdate {
            payload: MapUpdatePayload {
                current_location: location,
                region: current_location.clone(),
                explored: explored_locs,
                fog_bounds: None,
            },
            player_id: player_id.to_string(),
        });
        // Location change = meaningful narrative beat → advance display round
        turn_manager.advance_round();
        tracing::info!(
            new_round = turn_manager.round(),
            interaction = turn_manager.interaction(),
            "turn_manager.advance_round — location change"
        );
    }

    // Strip the location header from narration text if present
    let clean_narration = strip_location_header(narration_text);

    // Bug 17: Accumulate narration history for context on subsequent turns.
    // Truncate narrator response to ~300 chars to keep context bounded.
    let truncated_narration: String = clean_narration.chars().take(300).collect();
    narration_history.push(format!(
        "[{}] Action: {}\nNarrator: {}",
        char_name, effective_action, truncated_narration
    ));
    // Cap the buffer at 20 entries to prevent unbounded growth
    if narration_history.len() > 20 {
        narration_history.drain(..narration_history.len() - 20);
    }

    // Update NPC registry from structured narrator output (preferred) + regex fallback.
    // Structured extraction produces clean data; regex catches NPCs the narrator forgot to list.
    let turn_approx = turn_manager.interaction() as u32;
    if !result.npcs_present.is_empty() {
        tracing::info!(count = result.npcs_present.len(), "npc_registry.structured — updating from narrator JSON");
        for npc in &result.npcs_present {
            if npc.name.is_empty() { continue; }
            let name_lower = npc.name.to_lowercase();
            if let Some(entry) = npc_registry.iter_mut().find(|e| {
                e.name.to_lowercase() == name_lower
                    || e.name.to_lowercase().contains(&name_lower)
                    || name_lower.contains(&e.name.to_lowercase())
            }) {
                // Update existing — preserve identity, update last_seen
                entry.last_seen_turn = turn_approx;
                if !current_location.is_empty() {
                    entry.location = current_location.to_string();
                }
                // Upgrade name if structured version is more specific
                if npc.name.len() > entry.name.len() {
                    entry.name = npc.name.clone();
                }
                // Fill in missing fields from structured data
                if entry.pronouns.is_empty() && !npc.pronouns.is_empty() {
                    entry.pronouns = npc.pronouns.clone();
                }
                if entry.role.is_empty() && !npc.role.is_empty() {
                    entry.role = npc.role.clone();
                }
                if entry.appearance.is_empty() && !npc.appearance.is_empty() {
                    entry.appearance = npc.appearance.clone();
                }
            } else if npc.is_new {
                // New NPC — create entry
                npc_registry.push(NpcRegistryEntry {
                    name: npc.name.clone(),
                    pronouns: npc.pronouns.clone(),
                    role: npc.role.clone(),
                    age: String::new(),
                    appearance: npc.appearance.clone(),
                    location: current_location.to_string(),
                    last_seen_turn: turn_approx,
                });
                tracing::info!(name = %npc.name, pronouns = %npc.pronouns, role = %npc.role, "npc_registry.new — created from structured data");
            }
        }
    }
    // Regex fallback — catches NPCs the narrator forgot to list in the JSON block.
    // Include both discovered regions AND all cartography region names so that
    // location-derived words (e.g., "Tood" from "Tood's Dome") are never registered as NPCs.
    let mut all_location_names: Vec<String> = discovered_regions.clone();
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            for (region_id, _name_lower) in &ss.region_names {
                if !all_location_names.iter().any(|r| r == region_id) {
                    all_location_names.push(region_id.clone());
                }
            }
        }
    }
    let region_refs: Vec<&str> = all_location_names.iter().map(|s| s.as_str()).collect();
    update_npc_registry(
        npc_registry,
        &clean_narration,
        current_location,
        turn_approx,
        &region_refs,
    );
    tracing::debug!(
        npc_count = npc_registry.len(),
        "NPC registry updated from narration"
    );

    // Continuity validation — check narrator output against game state.
    // Build a minimal snapshot from the local session variables for the validator.
    {
        let mut validation_snapshot = sidequest_game::GameSnapshot {
            location: current_location.clone(),
            ..sidequest_game::GameSnapshot::default()
        };
        // Reconstruct character with inventory for the validator
        if let Some(ref cj) = character_json {
            if let Ok(mut ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
                ch.core.hp = *hp;
                ch.core.inventory = inventory.clone();
                validation_snapshot.characters.push(ch);
            }
        }
        let validation_result =
            sidequest_game::validate_continuity(&clean_narration, &validation_snapshot);
        if !validation_result.is_clean() {
            let corrections = validation_result.format_corrections();
            tracing::warn!(
                contradictions = validation_result.contradictions.len(),
                "continuity.contradictions_detected"
            );
            for c in &validation_result.contradictions {
                tracing::warn!(
                    category = ?c.category,
                    detail = %c.detail,
                    expected = %c.expected,
                    "continuity.contradiction"
                );
            }
            // Store for injection into the NEXT turn's narrator prompt
            *continuity_corrections = corrections;
        }
    }

    // Combat HP changes — apply typed CombatPatch from creature_smith (replaces keyword heuristic)
    if let Some(ref combat_patch) = result.combat_patch {
        if let Some(ref hp_changes) = combat_patch.hp_changes {
            let char_name_lower = player_name_for_save.to_lowercase();
            for (target, delta) in hp_changes {
                let target_lower = target.to_lowercase();
                if target_lower == char_name_lower
                    || character_json.as_ref().and_then(|cj| cj.get("name")).and_then(|n| n.as_str()).map(|n| n.to_lowercase() == target_lower).unwrap_or(false)
                {
                    *hp = sidequest_game::clamp_hp(*hp, *delta, *max_hp);
                    tracing::info!(target = %target, delta = delta, new_hp = *hp, "combat.patch.hp_applied");
                }
            }
        }
        if let Some(in_combat) = combat_patch.in_combat {
            if in_combat && !combat_state.in_combat() {
                combat_state.set_in_combat(true);
                tracing::info!("combat.patch.started");
            } else if !in_combat && combat_state.in_combat() {
                combat_state.set_in_combat(false);
                tracing::info!("combat.patch.ended");
            }
        }
        if let Some(dw) = combat_patch.drama_weight {
            combat_state.set_drama_weight(dw);
        }
        if combat_patch.advance_round {
            combat_state.advance_round();
        }
    }

    // Quest log updates — merge narrator-extracted quest changes
    if !result.quest_updates.is_empty() {
        for (quest_name, status) in &result.quest_updates {
            quest_log.insert(quest_name.clone(), status.clone());
            tracing::info!(quest = %quest_name, status = %status, "quest.updated");
        }
    }

    // Bug 3: XP award based on action type
    {
        let xp_award = if combat_state.in_combat() {
            25 // combat actions give more XP
        } else {
            10 // exploration/dialogue gives base XP
        };
        *xp += xp_award;
        tracing::info!(
            xp_award = xp_award,
            total_xp = *xp,
            level = *level,
            "XP awarded"
        );

        // Check for level up
        let threshold = sidequest_game::xp_for_level(*level + 1);
        if *xp >= threshold {
            *level += 1;
            let new_max_hp = sidequest_game::level_to_hp(10, *level);
            let hp_gain = new_max_hp - *max_hp;
            *max_hp = new_max_hp;
            *hp = sidequest_game::clamp_hp(*hp + hp_gain, 0, *max_hp);
            tracing::info!(
                new_level = *level,
                new_max_hp = *max_hp,
                hp_gain = hp_gain,
                "Level up!"
            );
        }
    }

    // Affinity progression (Story F8) — check thresholds after XP/level-up.
    // Loads genre pack affinities via state to avoid adding another parameter.
    if let Some(ref cj) = character_json {
        if let Ok(mut ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
            // Sync mutable fields
            ch.core.hp = *hp;
            ch.core.max_hp = *max_hp;
            ch.core.level = *level;
            ch.core.inventory = inventory.clone();

            // Increment affinity progress for any matching action triggers.
            let genre_code = sidequest_genre::GenreCode::new(genre_slug);
            if let Ok(code) = genre_code {
                let loader = GenreLoader::new(vec![state.genre_packs_path().to_path_buf()]);
                if let Ok(pack) = loader.load(&code) {
                    let genre_affinities = &pack.progression.affinities;

                    // Increment progress for affinities whose triggers match the action
                    for aff_def in genre_affinities {
                        let action_lower = effective_action.to_lowercase();
                        let matches_trigger = aff_def
                            .triggers
                            .iter()
                            .any(|t| action_lower.contains(&t.to_lowercase()));
                        if matches_trigger {
                            sidequest_game::increment_affinity_progress(
                                &mut ch.affinities,
                                &aff_def.name,
                                1,
                            );
                            tracing::info!(
                                affinity = %aff_def.name,
                                progress = ch.affinities.iter().find(|a| a.name == aff_def.name).map(|a| a.progress).unwrap_or(0),
                                "Affinity progress incremented"
                            );
                        }
                    }

                    // Check thresholds for tier-ups
                    let thresholds_for = |name: &str| -> Option<Vec<u32>> {
                        genre_affinities
                            .iter()
                            .find(|a| a.name == name)
                            .map(|a| a.tier_thresholds.clone())
                    };
                    let narration_hint_for = |name: &str, tier: u8| -> Option<String> {
                        genre_affinities
                            .iter()
                            .find(|a| a.name == name)
                            .and_then(|a| {
                                a.unlocks.as_ref().and_then(|u| {
                                    let tier_data = match tier {
                                        1 => u.tier_1.as_ref(),
                                        2 => u.tier_2.as_ref(),
                                        3 => u.tier_3.as_ref(),
                                        _ => None,
                                    };
                                    tier_data.map(|t| t.description.clone())
                                })
                            })
                    };

                    let tier_events = sidequest_game::check_affinity_thresholds(
                        &mut ch.affinities,
                        char_name,
                        &thresholds_for,
                        &narration_hint_for,
                    );

                    for event in &tier_events {
                        tracing::info!(
                            affinity = %event.affinity_name,
                            old_tier = event.old_tier,
                            new_tier = event.new_tier,
                            character = %event.character_name,
                            "Affinity tier up!"
                        );
                    }
                }
            } // if let Ok(code)

            // Write updated character back to character_json
            if let Ok(updated_json) = serde_json::to_value(&ch) {
                *character_json = Some(updated_json);
            }
        }
    }

    // Item acquisition — driven by structured extraction from the LLM response.
    // The narrator emits items_gained in its JSON block when the player
    // actually acquires something.
    const VALID_ITEM_CATEGORIES: &[&str] = &[
        "weapon", "armor", "tool", "consumable", "quest", "treasure", "misc",
    ];
    for item_def in &result.items_gained {
        // Reject prose fragments: item names should be short noun phrases,
        // not sentences or long descriptions.
        let name_trimmed = item_def.name.trim();
        let word_count = name_trimmed.split_whitespace().count();
        if name_trimmed.len() > 60 || word_count > 8 {
            tracing::warn!(
                item_name = %item_def.name,
                len = name_trimmed.len(),
                words = word_count,
                "Rejected item: name too long (likely prose fragment)"
            );
            continue;
        }
        // Reject names that look like sentences (contain common verbs)
        let lower = name_trimmed.to_lowercase();
        if lower.starts_with("the ") && word_count > 5 {
            tracing::warn!(item_name = %item_def.name, "Rejected item: sentence-like name");
            continue;
        }
        // Validate category
        let category = item_def.category.trim().to_lowercase();
        let valid_cat = if VALID_ITEM_CATEGORIES.contains(&category.as_str()) {
            category
        } else {
            "misc".to_string()
        };
        let item_id = name_trimmed
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        if inventory.find(&item_id).is_some() {
            continue;
        }
        if let (Ok(id), Ok(name), Ok(desc), Ok(cat), Ok(rarity)) = (
            sidequest_protocol::NonBlankString::new(&item_id),
            sidequest_protocol::NonBlankString::new(name_trimmed),
            sidequest_protocol::NonBlankString::new(&item_def.description),
            sidequest_protocol::NonBlankString::new(&valid_cat),
            sidequest_protocol::NonBlankString::new("common"),
        ) {
            let item = sidequest_game::Item {
                id,
                name,
                description: desc,
                category: cat,
                value: 0,
                weight: 1.0,
                rarity,
                narrative_weight: 0.3,
                tags: vec![],
                equipped: false,
                quantity: 1,
            };
            let _ = inventory.add(item, 50);
            tracing::info!(item_name = %item_def.name, "Item added to inventory from LLM extraction");
        }
    }

    // Legacy regex-based extraction disabled — replaced by LLM structured extraction above.
    if false {
        let items_found = extract_items_from_narration(&clean_narration);
        for (item_name, item_type) in &items_found {
            let item_id = item_name
                .to_lowercase()
                .replace(' ', "_")
                .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
            // Skip if already in inventory
            if inventory.find(&item_id).is_some() {
                continue;
            }
            if let (Ok(id), Ok(name), Ok(desc), Ok(cat), Ok(rarity)) = (
                sidequest_protocol::NonBlankString::new(&item_id),
                sidequest_protocol::NonBlankString::new(item_name),
                sidequest_protocol::NonBlankString::new(&format!(
                    "A {} found during adventure",
                    item_type
                )),
                sidequest_protocol::NonBlankString::new(item_type),
                sidequest_protocol::NonBlankString::new("common"),
            ) {
                let item = sidequest_game::Item {
                    id,
                    name,
                    description: desc,
                    category: cat,
                    value: 0,
                    weight: 1.0,
                    rarity,
                    narrative_weight: 0.3,
                    tags: vec![],
                    equipped: false,
                    quantity: 1,
                };
                let _ = inventory.add(item, 50);
                tracing::info!(item_name = %item_name, "Item added to inventory from narration");
                state.send_watcher_event(WatcherEvent {
                    timestamp: chrono::Utc::now(),
                    component: "inventory".to_string(),
                    event_type: WatcherEventType::StateTransition,
                    severity: Severity::Info,
                    fields: {
                        let mut f = HashMap::new();
                        f.insert("event".to_string(), serde_json::json!("item_gained"));
                        f.insert("item".to_string(), serde_json::json!(item_name));
                        f.insert(
                            "turn_number".to_string(),
                            serde_json::json!(turn_manager.interaction()),
                        );
                        f
                    },
                });
            }
        }

        // Extract item losses from narration (trades, gifts, drops)
        let items_lost = extract_item_losses(&clean_narration);
        for lost_name in &items_lost {
            let item_id = lost_name
                .to_lowercase()
                .replace(' ', "_")
                .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
            if inventory.find(&item_id).is_some() {
                let _ = inventory.remove(&item_id);
                tracing::info!(item_name = %lost_name, "Item removed from inventory from narration");
                state.send_watcher_event(WatcherEvent {
                    timestamp: chrono::Utc::now(),
                    component: "inventory".to_string(),
                    event_type: WatcherEventType::StateTransition,
                    severity: Severity::Info,
                    fields: {
                        let mut f = HashMap::new();
                        f.insert("event".to_string(), serde_json::json!("item_lost"));
                        f.insert("item".to_string(), serde_json::json!(lost_name));
                        f.insert(
                            "turn_number".to_string(),
                            serde_json::json!(turn_manager.interaction()),
                        );
                        f
                    },
                });
            }
        }
    }

    // Narration — include character state so the UI state mirror picks it up
    let inventory_names: Vec<String> = inventory
        .items
        .iter()
        .map(|i| i.name.as_str().to_string())
        .collect();
    let char_class_name = character_json
        .as_ref()
        .and_then(|cj| cj.get("char_class"))
        .and_then(|c| c.as_str())
        .unwrap_or("Adventurer");
    messages.push(GameMessage::Narration {
        payload: NarrationPayload {
            text: clean_narration.clone(),
            state_delta: Some(sidequest_protocol::StateDelta {
                location: extract_location_header(narration_text),
                characters: Some(vec![sidequest_protocol::CharacterState {
                    name: char_name.to_string(),
                    hp: *hp,
                    max_hp: *max_hp,
                    level: *level,
                    class: char_class_name.to_string(),
                    statuses: vec![],
                    inventory: inventory_names.clone(),
                }]),
                quests: if quest_log.is_empty() { None } else { Some(quest_log.clone()) },
                items_gained: if result.items_gained.is_empty() {
                    None
                } else {
                    Some(result.items_gained.clone())
                },
            }),
            footnotes: result.footnotes.clone(),
        },
        player_id: player_id.to_string(),
    });

    // RAG pipeline: convert new footnotes to discovered facts (story 9-11)
    if !result.footnotes.is_empty() {
        let discovered = sidequest_agents::footnotes::footnotes_to_discovered_facts(
            &result.footnotes,
            char_name,
            turn_manager.interaction(),
        );
        if !discovered.is_empty() {
            tracing::info!(
                count = discovered.len(),
                character = %char_name,
                interaction = turn_manager.interaction(),
                "rag.footnotes_to_discovered_facts"
            );
            // Apply discovered facts to snapshot via WorldStatePatch path
            // (This feeds into the persistence layer on next save)
        }
    }

    // Narration end with state_delta field present (even if empty)
    messages.push(GameMessage::NarrationEnd {
        payload: NarrationEndPayload {
            state_delta: Some(sidequest_protocol::StateDelta {
                location: None,
                characters: None,
                quests: None,
                items_gained: None,
            }),
        },
        player_id: player_id.to_string(),
    });

    // Extract character class from JSON for PartyStatus
    let char_class = character_json
        .as_ref()
        .and_then(|cj| cj.get("char_class"))
        .and_then(|c| c.as_str())
        .unwrap_or("Adventurer");

    // Party status — build full party from shared session (multiplayer) or local only (single-player)
    {
        let mut party_members = vec![PartyMember {
            player_id: player_id.to_string(),
            name: player_name_for_save.to_string(),
            character_name: char_name.to_string(),
            current_hp: *hp,
            max_hp: *max_hp,
            statuses: vec![],
            class: char_class.to_string(),
            level: *level,
            portrait_url: None,
        }];
        // In multiplayer, include other players from the shared session
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            for (pid, ps) in &ss.players {
                if pid == player_id {
                    continue; // Already added above with fresh local data
                }
                party_members.push(PartyMember {
                    player_id: pid.clone(),
                    name: ps.player_name.clone(),
                    character_name: ps.character_name.clone().unwrap_or_else(|| ps.player_name.clone()),
                    current_hp: ps.character_hp,
                    max_hp: ps.character_max_hp,
                    statuses: vec![],
                    class: String::new(),
                    level: ps.character_level,
                    portrait_url: None,
                });
            }
        }
        messages.push(GameMessage::PartyStatus {
            payload: PartyStatusPayload {
                members: party_members,
            },
            player_id: player_id.to_string(),
        });
    }

    // Bug 5: Inventory — now wired to actual inventory state
    messages.push(GameMessage::Inventory {
        payload: InventoryPayload {
            items: inventory
                .items
                .iter()
                .map(|item| sidequest_protocol::InventoryItem {
                    name: item.name.as_str().to_string(),
                    item_type: item.category.as_str().to_string(),
                    equipped: item.equipped,
                    quantity: item.quantity,
                    description: item.description.as_str().to_string(),
                })
                .collect(),
            gold: inventory.gold,
        },
        player_id: player_id.to_string(),
    });

    // Combat detection — intent-based (primary) + keyword scan (fallback).
    // If the intent classifier routed to creature_smith, that's a combat action.
    if !combat_state.in_combat() {
        if let Some(ref intent) = result.classified_intent {
            if intent == "Combat" {
                combat_state.set_in_combat(true);
                tracing::info!(intent = %intent, agent = ?result.agent_name, "combat.started — intent classifier triggered combat state");
                {
                    let holder = shared_session_holder.lock().await;
                    if let Some(ref ss_arc) = *holder {
                        let mut ss = ss_arc.lock().await;
                        let old_mode = std::mem::take(&mut ss.turn_mode);
                        ss.turn_mode = old_mode
                            .apply(sidequest_game::turn_mode::TurnModeTransition::CombatStarted);
                    }
                }
            }
        }
    }

    // Keyword-based combat detection — fallback for cases where intent
    // classification missed but narration clearly describes combat.
    {
        let narr_lower = clean_narration.to_lowercase();
        let combat_start_keywords = [
            "initiative",
            "combat begins",
            "roll for initiative",
            "attacks you",
            "lunges at",
            "swings at",
            "draws a weapon",
            "charges at",
            "opens fire",
            "enters combat",
        ];
        let combat_end_keywords = [
            "combat ends",
            "battle is over",
            "enemies defeated",
            "falls unconscious",
            "retreats",
            "flees",
            "surrenders",
            "combat resolved",
            "the fight is over",
        ];

        if combat_state.in_combat() {
            // Check for combat end
            if combat_end_keywords.iter().any(|kw| narr_lower.contains(kw)) {
                combat_state.set_in_combat(false);
                tracing::info!("Combat ended — detected end keyword in narration");
                // Transition turn mode: Structured → FreePlay
                {
                    let holder = shared_session_holder.lock().await;
                    if let Some(ref ss_arc) = *holder {
                        let mut ss = ss_arc.lock().await;
                        let old_mode = std::mem::take(&mut ss.turn_mode);
                        ss.turn_mode = old_mode
                            .apply(sidequest_game::turn_mode::TurnModeTransition::CombatEnded);
                        tracing::info!(new_mode = ?ss.turn_mode, "Turn mode transitioned on combat end");
                    }
                }
            }
        } else {
            // Check for combat start
            if combat_start_keywords
                .iter()
                .any(|kw| narr_lower.contains(kw))
            {
                combat_state.set_in_combat(true);
                tracing::info!("Combat started — detected start keyword in narration");
                // Transition turn mode: FreePlay → Structured
                {
                    let holder = shared_session_holder.lock().await;
                    if let Some(ref ss_arc) = *holder {
                        let mut ss = ss_arc.lock().await;
                        let old_mode = std::mem::take(&mut ss.turn_mode);
                        ss.turn_mode = old_mode
                            .apply(sidequest_game::turn_mode::TurnModeTransition::CombatStarted);
                        tracing::info!(new_mode = ?ss.turn_mode, "Turn mode transitioned on combat start");
                        // Initialize barrier if transitioning to structured mode
                        if ss.turn_mode.should_use_barrier() && ss.turn_barrier.is_none() {
                            let mp_session = sidequest_game::multiplayer::MultiplayerSession::with_player_ids(
                                ss.players.keys().cloned(),
                            );
                            let adaptive = sidequest_game::barrier::AdaptiveTimeout::default();
                            ss.turn_barrier = Some(sidequest_game::barrier::TurnBarrier::with_adaptive(
                                mp_session,
                                adaptive,
                            ));
                        }
                    }
                }
            }
        }
    }

    // Combat tick — uses persistent per-session CombatState
    let was_in_combat = combat_state.in_combat();
    tracing::debug!(
        in_combat = was_in_combat,
        round = combat_state.round(),
        drama_weight = combat_state.drama_weight(),
        "combat.pre_tick"
    );
    if combat_state.in_combat() {
        combat_state.tick_effects();
        combat_state.advance_round();
        state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "combat".to_string(),
            event_type: WatcherEventType::AgentSpanOpen,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert("round".to_string(), serde_json::json!(combat_state.round()));
                f.insert(
                    "drama_weight".to_string(),
                    serde_json::json!(combat_state.drama_weight()),
                );
                f
            },
        });
    }

    // Combat overlay — send whenever combat state is relevant
    if was_in_combat || combat_state.in_combat() {
        messages.push(GameMessage::CombatEvent {
            payload: CombatEventPayload {
                in_combat: combat_state.in_combat(),
                enemies: vec![],
                turn_order: vec![],
                current_turn: String::new(),
            },
            player_id: player_id.to_string(),
        });
    }

    // Bug 6: Chase detection and state tracking
    {
        let narr_lower = clean_narration.to_lowercase();
        let chase_start_keywords = [
            "chase begins",
            "gives chase",
            "starts chasing",
            "run!",
            "flee!",
            "pursues you",
            "pursuit begins",
            "races after",
            "sprints after",
            "bolts away",
        ];
        let chase_end_keywords = [
            "escape",
            "lost them",
            "chase ends",
            "caught up",
            "stopped running",
            "pursuit ends",
            "safe now",
            "shakes off",
            "outrun",
        ];

        if let Some(ref mut cs) = chase_state {
            // Update active chase
            if chase_end_keywords.iter().any(|kw| narr_lower.contains(kw)) {
                tracing::info!(rounds = cs.round(), "Chase resolved");
                *chase_state = None;
            } else {
                // Advance chase round, adjust separation based on narration
                let gain = if narr_lower.contains("gaining") || narr_lower.contains("closing") {
                    -1
                } else if narr_lower.contains("widening") || narr_lower.contains("pulling ahead") {
                    1
                } else {
                    0
                };
                cs.set_separation(cs.separation() + gain);
                cs.record_roll(0.5); // placeholder roll
                tracing::info!(round = cs.round(), separation = cs.separation(), gain, "chase.tick — round advanced");
            }
        } else if chase_start_keywords
            .iter()
            .any(|kw| narr_lower.contains(kw))
        {
            let cs = sidequest_game::ChaseState::new(sidequest_game::ChaseType::Footrace, 0.5);
            tracing::info!("Chase started — detected chase keyword in narration");
            *chase_state = Some(cs);
        }
    }

    // Scan narration for trope trigger keywords → activate matching tropes
    let narration_lower = clean_narration.to_lowercase();
    tracing::debug!(
        narration_len = narration_lower.len(),
        active_tropes = trope_states.len(),
        total_defs = trope_defs.len(),
        "Trope keyword scan starting"
    );
    for def in trope_defs.iter() {
        let id = match &def.id {
            Some(id) => id,
            None => continue,
        };
        // Skip already active tropes
        if trope_states.iter().any(|ts| ts.trope_definition_id() == id) {
            continue;
        }
        // Check if any trigger keyword appears in the narration
        let triggered = def
            .triggers
            .iter()
            .any(|t| narration_lower.contains(&t.to_lowercase()));
        if triggered {
            sidequest_game::trope::TropeEngine::activate(trope_states, id);
            tracing::info!(trope_id = %id, "Trope activated by narration keyword");
            state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "trope".to_string(),
                event_type: WatcherEventType::StateTransition,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert(
                        "event".to_string(),
                        serde_json::Value::String("trope_activated".to_string()),
                    );
                    f.insert(
                        "trope_id".to_string(),
                        serde_json::Value::String(id.clone()),
                    );
                    f.insert(
                        "trigger".to_string(),
                        serde_json::Value::String("narration_keyword".to_string()),
                    );
                    f
                },
            });
        }
    }

    // Trope engine tick — uses persistent per-session trope state and genre pack defs
    // Log pre-tick state for debugging
    for ts in trope_states.iter() {
        tracing::info!(
            trope_id = %ts.trope_definition_id(),
            status = ?ts.status(),
            progression = ts.progression(),
            fired_beats = ts.fired_beats().len(),
            "Trope pre-tick state"
        );
    }
    let fired = sidequest_game::trope::TropeEngine::tick(trope_states, trope_defs);
    sidequest_game::trope::TropeEngine::apply_keyword_modifiers(
        trope_states,
        trope_defs,
        &clean_narration,
    );
    tracing::info!(
        active_tropes = trope_states.len(),
        fired_beats = fired.len(),
        "Trope tick complete"
    );
    // Log post-tick state
    for ts in trope_states.iter() {
        tracing::debug!(
            trope_id = %ts.trope_definition_id(),
            status = ?ts.status(),
            progression = ts.progression(),
            "Trope post-tick state"
        );
    }
    for beat in &fired {
        tracing::info!(trope = %beat.trope_name, "Trope beat fired");
        state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "trope".to_string(),
            event_type: WatcherEventType::AgentSpanOpen,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert(
                    "trope".to_string(),
                    serde_json::Value::String(beat.trope_name.clone()),
                );
                f.insert(
                    "trope_id".to_string(),
                    serde_json::Value::String(beat.trope_id.clone()),
                );
                f
            },
        });
    }

    // Render pipeline — extract subject from narration, filter, enqueue
    let extraction_context = sidequest_game::ExtractionContext {
        current_location: extract_location_header(narration_text).unwrap_or_default(),
        in_combat: combat_state.in_combat(),
        known_npcs: npc_registry.iter().map(|e| e.name.clone()).collect(),
        ..Default::default()
    };
    if let Some(subject) = state
        .subject_extractor()
        .extract(&clean_narration, &extraction_context)
    {
        tracing::info!(
            prompt = %subject.prompt_fragment(),
            tier = ?subject.tier(),
            weight = subject.narrative_weight(),
            "Subject extracted from narration"
        );
        let filter_ctx = sidequest_game::FilterContext {
            in_combat: combat_state.in_combat(),
            scene_transition: extract_location_header(narration_text).is_some(),
            player_requested: false,
        };
        let decision = state
            .beat_filter()
            .lock()
            .await
            .evaluate(&subject, &filter_ctx);
        tracing::info!(decision = ?decision, "BeatFilter decision");
        if matches!(decision, sidequest_game::FilterDecision::Render { .. }) {
            if let Some(queue) = state.render_queue() {
                // Compose the full style string: location tag override + positive_suffix.
                // This flows through the render queue as "art_style" and gets combined
                // with the raw prompt fragment in the render closure to build positive_prompt.
                let (art_style, model, neg_prompt) = match visual_style {
                    Some(ref vs) => {
                        // Match visual_tag_overrides against current location (substring match)
                        let location = extraction_context.current_location.to_lowercase();
                        let tag_override = if !location.is_empty() {
                            vs.visual_tag_overrides
                                .iter()
                                .find(|(key, _)| location.contains(key.as_str()))
                                .map(|(_, val)| val.as_str())
                        } else {
                            None
                        };
                        let style = match tag_override {
                            Some(tag) => format!("{}, {}", tag, vs.positive_suffix),
                            None => vs.positive_suffix.clone(),
                        };
                        (style, vs.preferred_model.clone(), vs.negative_prompt.clone())
                    }
                    None => ("oil_painting".to_string(), "flux-schnell".to_string(), String::new()),
                };
                match queue.enqueue(subject, &art_style, &model, &neg_prompt).await {
                    Ok(result) => tracing::info!(result = ?result, "Render job enqueued"),
                    Err(e) => tracing::warn!(error = %e, "Render enqueue failed"),
                }
            }
        }
    } else {
        tracing::debug!(
            narration_len = clean_narration.len(),
            "No render subject extracted"
        );
    }

    // Audio cue — evaluate mood via MusicDirector, route through AudioMixer
    if let Some(ref mut director) = music_director {
        tracing::info!("music_director_present — evaluating mood");
        let mood_ctx = sidequest_game::MoodContext {
            in_combat: combat_state.in_combat(),
            in_chase: chase_state.is_some(),
            party_health_pct: if *max_hp > 0 {
                *hp as f32 / *max_hp as f32
            } else {
                1.0
            },
            quest_completed: {
                let narr = clean_narration.to_lowercase();
                narr.contains("quest complete") || narr.contains("mission accomplished")
                    || narr.contains("task done") || narr.contains("objective achieved")
            },
            npc_died: {
                let narr = clean_narration.to_lowercase();
                narr.contains("falls dead") || narr.contains("killed")
                    || narr.contains("dies") || narr.contains("slain")
                    || narr.contains("collapses lifeless")
            },
        };
        // Classify mood first so we can include it in the protocol message
        let classification = director.classify_mood(&clean_narration, &mood_ctx);
        let mood_key = classification.primary.as_key();
        tracing::info!(
            mood = mood_key,
            intensity = classification.intensity,
            confidence = classification.confidence,
            in_combat = mood_ctx.in_combat,
            "music_mood_classified"
        );
        if let Some(cue) = director.evaluate(&clean_narration, &mood_ctx) {
            tracing::info!(
                mood = mood_key,
                track = ?cue.track_id,
                action = %cue.action,
                volume = cue.volume,
                "music_cue_produced"
            );
            let mixer_cues = {
                let mut mixer_guard = audio_mixer.lock().await;
                if let Some(ref mut mixer) = *mixer_guard {
                    mixer.apply_cue(cue)
                } else {
                    vec![cue]
                }
            };
            tracing::info!(cue_count = mixer_cues.len(), "music_mixer_cues_ready");
            for c in &mixer_cues {
                messages.push(audio_cue_to_game_message(
                    c,
                    player_id,
                    genre_slug,
                    Some(mood_key),
                ));
            }
        } else {
            tracing::warn!(
                mood = mood_key,
                "music_evaluate_returned_none — no cue produced"
            );
        }
    } else {
        tracing::warn!("music_director_missing — audio cues skipped");
    }

    // Record this interaction in the turn manager (granular counter for chronology)
    turn_manager.record_interaction();
    tracing::info!(
        interaction = turn_manager.interaction(),
        round = turn_manager.round(),
        "turn_manager.record_interaction"
    );

    // Persist updated game state (location, narration log) for reconnection
    if !genre_slug.is_empty() && !world_slug.is_empty() {
        let location =
            extract_location_header(narration_text).unwrap_or_else(|| "Starting area".to_string());
        match state
            .persistence()
            .load(genre_slug, world_slug, player_name_for_save)
            .await
        {
            Ok(Some(saved)) => {
                let mut snapshot = saved.snapshot;
                snapshot.location = location;
                // Sync ALL game state to snapshot for persistence
                snapshot.turn_manager = turn_manager.clone();
                snapshot.npc_registry = npc_registry.clone();
                snapshot.axis_values = axis_values.clone();
                snapshot.combat = combat_state.clone();
                snapshot.chase = chase_state.clone();
                snapshot.discovered_regions = discovered_regions.clone();
                snapshot.active_tropes = trope_states.clone();
                snapshot.quest_log = quest_log.clone();
                // Sync character state (HP, XP, level, inventory, known_facts, affinities)
                if let Some(ref cj) = character_json {
                    if let Ok(ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
                        if let Some(saved_ch) = snapshot.characters.first_mut() {
                            saved_ch.core.hp = *hp;
                            saved_ch.core.max_hp = *max_hp;
                            saved_ch.core.level = *level;
                            saved_ch.core.inventory = inventory.clone();
                            saved_ch.known_facts = ch.known_facts.clone();
                            saved_ch.affinities = ch.affinities.clone();
                            saved_ch.narrative_state = ch.narrative_state.clone();
                        }
                    }
                }
                // Append narration to log for recap on reconnect
                snapshot.narrative_log.push(sidequest_game::NarrativeEntry {
                    timestamp: 0,
                    round: turn_manager.interaction() as u32,
                    author: "narrator".to_string(),
                    content: clean_narration.clone(),
                    tags: vec![],
                    encounter_tags: vec![],
                    speaker: None,
                    entry_type: None,
                });
                match state
                    .persistence()
                    .save(genre_slug, world_slug, player_name_for_save, &snapshot)
                    .await
                {
                    Ok(_) => tracing::info!(
                        player = %player_name_for_save,
                        turn = turn_manager.interaction(),
                        location = %current_location,
                        hp = *hp,
                        items = inventory.items.len(),
                        "session.saved — game state persisted"
                    ),
                    Err(e) => tracing::warn!(error = %e, "Failed to persist updated game state"),
                }
            }
            Ok(None) => {
                tracing::debug!("No saved session to update");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load session for persistence update");
            }
        }
    }

    // TTS streaming — segment narration and spawn background synthesis task
    if !clean_narration.is_empty() && !state.tts_disabled() {
        let segmenter = sidequest_game::SentenceSegmenter::new();
        let segments = segmenter.segment(&clean_narration);
        tracing::info!(
            segment_count = segments.len(),
            narration_len = clean_narration.len(),
            "tts.segmented"
        );
        if !segments.is_empty() {
            let tts_segments: Vec<sidequest_game::tts_stream::TtsSegment> = segments
                .iter()
                .map(|seg| sidequest_game::tts_stream::TtsSegment {
                    text: strip_markdown_for_tts(&seg.text),
                    index: seg.index,
                    is_last: seg.is_last,
                    speaker: "narrator".to_string(),
                    pause_after_ms: if seg.is_last { 0 } else { 200 },
                })
                .collect();

            let player_id_for_tts = player_id.to_string();
            let state_for_tts = state.clone();
            let ss_holder_for_tts = shared_session_holder.clone();
            let tts_config = sidequest_game::tts_stream::TtsStreamConfig::default();
            let streamer = sidequest_game::tts_stream::TtsStreamer::new(tts_config);

            // Clone Arcs for the spawned TTS task (mixer ducking + prerender)
            let mixer_for_tts = std::sync::Arc::clone(audio_mixer);
            let prerender_for_tts = std::sync::Arc::clone(prerender_scheduler);
            let genre_slug_for_tts = genre_slug.to_string();
            let tts_segments_for_prerender = tts_segments.clone();
            let prerender_ctx = sidequest_game::PrerenderContext {
                in_combat: combat_state.in_combat(),
                combatant_names: if combat_state.in_combat() {
                    result.npcs_present.iter().map(|npc| npc.name.clone()).collect()
                } else {
                    vec![]
                },
                pending_destination: extract_location_header(narration_text).map(|s| s.to_string()),
                active_dialogue_npc: npc_registry.last().map(|e| e.name.clone()),
                art_style: match visual_style {
                    Some(ref vs) => vs.positive_suffix.clone(),
                    None => "oil_painting".to_string(),
                },
                negative_prompt: match visual_style {
                    Some(ref vs) => vs.negative_prompt.clone(),
                    None => String::new(),
                },
            };

            tokio::spawn(async move {
                let (msg_tx, mut msg_rx) =
                    tokio::sync::mpsc::channel::<sidequest_game::tts_stream::TtsMessage>(32);

                // Connect to daemon for synthesis
                let daemon_config = sidequest_daemon_client::DaemonConfig::default();
                let synthesizer = match sidequest_daemon_client::DaemonClient::connect(
                    daemon_config,
                )
                .await
                {
                    Ok(client) => DaemonSynthesizer {
                        client: tokio::sync::Mutex::new(client),
                    },
                    Err(e) => {
                        tracing::warn!(error = %e, "TTS daemon unavailable — skipping voice synthesis");
                        return;
                    }
                };

                // Spawn the streamer pipeline
                let stream_handle = tokio::spawn(async move {
                    if let Err(e) = streamer.stream(tts_segments, &synthesizer, msg_tx).await {
                        tracing::warn!(error = %e, "TTS stream failed");
                    }
                });

                // Helper: send a game message to the acting player only.
                // In multiplayer, routes via session channel. Single-player falls back to global broadcast.
                let send_to_acting_player = |msg: GameMessage, ss_holder: &Arc<tokio::sync::Mutex<Option<Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>>, pid: &str, fallback_state: &AppState| {
                    let ss_holder = ss_holder.clone();
                    let pid = pid.to_string();
                    let fallback_state = fallback_state.clone();
                    let msg = msg.clone();
                    let msg_type = format!("{:?}", std::mem::discriminant(&msg));
                    tokio::spawn(async move {
                        let holder = ss_holder.lock().await;
                        if let Some(ref ss_arc) = *holder {
                            let ss = ss_arc.lock().await;
                            tracing::debug!(player_id = %pid, msg_type = %msg_type, "tts.send_to_acting_player — via session channel");
                            ss.send_to_player(msg, pid);
                        } else {
                            tracing::debug!(player_id = %pid, msg_type = %msg_type, "tts.send_to_acting_player — via global broadcast (single-player)");
                            let _ = fallback_state.broadcast(msg);
                        }
                    });
                };

                // Bridge TtsMessage → binary frames (chunks) or GameMessage (start/end)
                while let Some(tts_msg) = msg_rx.recv().await {
                    match tts_msg {
                        sidequest_game::tts_stream::TtsMessage::Start { total_segments } => {
                            // Duck audio channels during TTS — audio cues go to acting player only
                            {
                                let mut mixer_guard = mixer_for_tts.lock().await;
                                if let Some(ref mut mixer) = *mixer_guard {
                                    for duck_cue in mixer.on_tts_start() {
                                        send_to_acting_player(
                                            audio_cue_to_game_message(
                                                &duck_cue,
                                                &player_id_for_tts,
                                                &genre_slug_for_tts,
                                                None,
                                            ),
                                            &ss_holder_for_tts,
                                            &player_id_for_tts,
                                            &state_for_tts,
                                        );
                                    }
                                }
                            }
                            // Speculative prerender during TTS playback
                            {
                                let mut prerender_guard = prerender_for_tts.lock().await;
                                if let Some(ref mut prerender) = *prerender_guard {
                                    if let Some(subject) = prerender
                                        .on_tts_start(&tts_segments_for_prerender, &prerender_ctx)
                                    {
                                        if let Some(queue) = state_for_tts.render_queue() {
                                            let _ = queue
                                                .enqueue(
                                                    subject,
                                                    &prerender_ctx.art_style,
                                                    "flux-schnell",
                                                    &prerender_ctx.negative_prompt,
                                                )
                                                .await;
                                        }
                                    }
                                }
                            }
                            let game_msg = GameMessage::TtsStart {
                                payload: sidequest_protocol::TtsStartPayload { total_segments },
                                player_id: player_id_for_tts.clone(),
                            };
                            send_to_acting_player(game_msg, &ss_holder_for_tts, &player_id_for_tts, &state_for_tts);
                        }
                        sidequest_game::tts_stream::TtsMessage::Chunk(chunk) => {
                            // Send NARRATION_CHUNK to acting player only (not global broadcast).
                            // Text reveals sentence-by-sentence synchronized with TTS playback.
                            if let Some(seg) = tts_segments_for_prerender.get(chunk.segment_index) {
                                let chunk_msg = GameMessage::NarrationChunk {
                                    payload: sidequest_protocol::NarrationChunkPayload {
                                        text: seg.text.clone(),
                                    },
                                    player_id: player_id_for_tts.clone(),
                                };
                                send_to_acting_player(chunk_msg, &ss_holder_for_tts, &player_id_for_tts, &state_for_tts);
                            }

                            // Build binary voice frame: [4-byte header len][JSON header][audio bytes]
                            // The daemon always returns raw PCM s16le — use that format string
                            // so the UI routes to playVoicePCM instead of decodeAudioData.
                            // NOTE: binary frames still use global broadcast — binary channel
                            // doesn't support per-player targeting yet.
                            let header = serde_json::json!({
                                "type": "VOICE_AUDIO",
                                "segment_id": format!("seg_{}", chunk.segment_index),
                                "sample_rate": 24000,
                                "format": "pcm_s16le"
                            });
                            let header_bytes = serde_json::to_vec(&header).unwrap_or_default();
                            let audio_bytes = &chunk.audio_raw;
                            let mut frame =
                                Vec::with_capacity(4 + header_bytes.len() + audio_bytes.len());
                            frame.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
                            frame.extend_from_slice(&header_bytes);
                            frame.extend_from_slice(audio_bytes);
                            state_for_tts.broadcast_binary(frame);
                        }
                        sidequest_game::tts_stream::TtsMessage::End => {
                            // Restore audio channels after TTS — acting player only
                            {
                                let mut mixer_guard = mixer_for_tts.lock().await;
                                if let Some(ref mut mixer) = *mixer_guard {
                                    for restore_cue in mixer.on_tts_end() {
                                        send_to_acting_player(
                                            audio_cue_to_game_message(
                                                &restore_cue,
                                                &player_id_for_tts,
                                                &genre_slug_for_tts,
                                                None,
                                            ),
                                            &ss_holder_for_tts,
                                            &player_id_for_tts,
                                            &state_for_tts,
                                        );
                                    }
                                }
                            }
                            // Clear prerender pending state
                            {
                                let mut prerender_guard = prerender_for_tts.lock().await;
                                if let Some(ref mut prerender) = *prerender_guard {
                                    prerender.on_tts_end();
                                }
                            }
                            let game_msg = GameMessage::TtsEnd {
                                player_id: player_id_for_tts.clone(),
                            };
                            send_to_acting_player(game_msg, &ss_holder_for_tts, &player_id_for_tts, &state_for_tts);
                        }
                    }
                }

                let _ = stream_handle.await;
                tracing::info!(player_id = %player_id_for_tts, "TTS stream complete");
            });
        }
    }

    // GM Panel: emit full game state snapshot after all mutations
    {
        let turn_approx = turn_manager.interaction() as u32;
        let npc_data: Vec<serde_json::Value> = npc_registry
            .iter()
            .map(|e| {
                serde_json::json!({
                    "name": e.name,
                    "pronouns": e.pronouns,
                    "role": e.role,
                    "location": e.location,
                    "last_seen_turn": e.last_seen_turn,
                })
            })
            .collect();
        let inventory_names: Vec<String> = inventory
            .items
            .iter()
            .map(|i| i.name.as_str().to_string())
            .collect();
        let active_tropes: Vec<serde_json::Value> = trope_states
            .iter()
            .map(|ts| {
                serde_json::json!({
                    "id": ts.trope_definition_id(),
                    "progression": ts.progression(),
                    "status": format!("{:?}", ts.status()),
                })
            })
            .collect();
        state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "game".to_string(),
            event_type: WatcherEventType::StateTransition,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert(
                    "event".to_string(),
                    serde_json::json!("game_state_snapshot"),
                );
                f.insert("turn_number".to_string(), serde_json::json!(turn_approx));
                f.insert(
                    "location".to_string(),
                    serde_json::json!(current_location.as_str()),
                );
                f.insert("hp".to_string(), serde_json::json!(*hp));
                f.insert("max_hp".to_string(), serde_json::json!(*max_hp));
                f.insert("level".to_string(), serde_json::json!(*level));
                f.insert("xp".to_string(), serde_json::json!(*xp));
                f.insert("inventory".to_string(), serde_json::json!(inventory_names));
                f.insert("npc_registry".to_string(), serde_json::json!(npc_data));
                f.insert(
                    "active_tropes".to_string(),
                    serde_json::json!(active_tropes),
                );
                f.insert(
                    "in_combat".to_string(),
                    serde_json::json!(combat_state.in_combat()),
                );
                f.insert("player_id".to_string(), serde_json::json!(player_id));
                f.insert("character".to_string(), serde_json::json!(char_name));
                f
            },
        });
    }

    // Sync world-level state back to shared session and broadcast narration
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let mut ss = ss_arc.lock().await;
            ss.sync_from_locals(
                current_location,
                npc_registry,
                narration_history,
                discovered_regions,
                trope_states,
                player_id,
            );
            // Sync acting player's character data to PlayerState for other players' PARTY_STATUS
            if let Some(ps) = ss.players.get_mut(player_id) {
                ps.character_hp = *hp;
                ps.character_max_hp = *max_hp;
                ps.character_level = *level;
                ps.character_xp = *xp;
                ps.character_class = char_class.to_string();
                ps.inventory = inventory.clone();
                ps.combat_state = combat_state.clone();
                ps.chase_state = chase_state.clone();
                if ps.character_name.is_none() {
                    ps.character_name = Some(char_name.to_string());
                }
            }
            // Route messages to session members.
            // The acting player already receives via their direct tx channel (mpsc).
            // Other players get narration (without state_delta) via the session broadcast channel.
            // Fall back to all session members when cartography regions aren't available.
            let co_located = ss.co_located_players(player_id);
            let other_players: Vec<String> = if co_located.is_empty() {
                // No region data — fall back to all other session members
                ss.players.keys().filter(|pid| pid.as_str() != player_id).cloned().collect()
            } else {
                co_located
            };
            for msg in &messages {
                match msg {
                    GameMessage::Narration { payload, .. } => {
                        // Send the acting player's action to observers FIRST.
                        // This creates a turn boundary in NarrativeView (PLAYER_ACTION triggers flushChunks).
                        let observer_action = GameMessage::PlayerAction {
                            payload: sidequest_protocol::PlayerActionPayload {
                                action: format!("{} — {}", char_name, effective_action),
                                aside: false,
                            },
                            player_id: player_id.to_string(),
                        };
                        tracing::info!(
                            char_name = %char_name,
                            observer_count = other_players.len(),
                            "multiplayer.observer_action — broadcasting PLAYER_ACTION to observers"
                        );
                        for target_id in &other_players {
                            ss.send_to_player(observer_action.clone(), target_id.clone());
                        }
                        // Send narration (state_delta stripped) to other players.
                        // Apply perception filters if active.
                        for target_id in &other_players {
                            let text = if let Some(filter) = ss.perception_filters.get(target_id) {
                                let effects_desc = sidequest_game::perception::PerceptionRewriter::describe_effects(filter.effects());
                                format!(
                                    "[Your perception is altered: {}]\n\n{}",
                                    effects_desc, payload.text
                                )
                            } else {
                                payload.text.clone()
                            };
                            let narration_msg = GameMessage::Narration {
                                payload: sidequest_protocol::NarrationPayload {
                                    text,
                                    state_delta: None,
                                    footnotes: payload.footnotes.clone(),
                                },
                                player_id: target_id.clone(),
                            };
                            ss.send_to_player(narration_msg, target_id.clone());
                        }
                        tracing::info!(
                            observer_count = other_players.len(),
                            text_len = payload.text.len(),
                            "multiplayer.narration_broadcast — sent to observers via session channel"
                        );
                    }
                    GameMessage::NarrationEnd { .. } => {
                        // Broadcast NarrationEnd to all players so TTS sync works correctly
                        let player_ids: Vec<String> = ss.players.keys().cloned().collect();
                        for target_pid in &player_ids {
                            let end_msg = GameMessage::NarrationEnd {
                                payload: NarrationEndPayload { state_delta: None },
                                player_id: target_pid.clone(),
                            };
                            ss.send_to_player(end_msg, target_pid.clone());
                        }
                        // TURN_STATUS "resolved" — unlock input for all players after narration completes.
                        // Use global broadcast (not session channel) for reliability — session
                        // subscribers may miss messages sent before subscription.
                        if ss.players.len() > 1 {
                            let resolved_msg = GameMessage::TurnStatus {
                                payload: TurnStatusPayload {
                                    player_name: player_name_for_save.to_string(),
                                    status: "resolved".into(),
                                    state_delta: None,
                                },
                                player_id: player_id.to_string(),
                            };
                            let _ = state.broadcast(resolved_msg);
                            tracing::info!(player_name = %player_name_for_save, "turn_status.resolved broadcast to all clients");
                        }
                    }
                    GameMessage::ChapterMarker { ref payload, .. } => {
                        // Send to other players only — acting player already received via direct channel
                        for target_pid in ss.players.keys().filter(|pid| pid.as_str() != player_id) {
                            let marker = GameMessage::ChapterMarker {
                                payload: payload.clone(),
                                player_id: target_pid.clone(),
                            };
                            ss.send_to_player(marker, target_pid.clone());
                        }
                    }
                    GameMessage::PartyStatus { .. } => {
                        // Build targeted PARTY_STATUS per player so every player's
                        // player_id is set correctly (client HUD uses this for identity).
                        let members: Vec<PartyMember> = ss
                            .players
                            .iter()
                            .map(|(pid, ps)| PartyMember {
                                player_id: pid.clone(),
                                name: ps.player_name.clone(),
                                character_name: ps.character_name.clone().unwrap_or_else(|| ps.player_name.clone()),
                                current_hp: ps.character_hp,
                                max_hp: ps.character_max_hp,
                                statuses: vec![],
                                class: ps.character_class.clone(),
                                level: ps.character_level,
                                portrait_url: None,
                            })
                            .collect();
                        let player_ids: Vec<String> = ss.players.keys().cloned().collect();
                        for target_pid in &player_ids {
                            let party_msg = GameMessage::PartyStatus {
                                payload: PartyStatusPayload { members: members.clone() },
                                player_id: target_pid.clone(),
                            };
                            ss.send_to_player(party_msg, target_pid.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    messages
}

/// DaemonSynthesizer — implements TtsSynthesizer for the real daemon client.
struct DaemonSynthesizer {
    client: tokio::sync::Mutex<sidequest_daemon_client::DaemonClient>,
}

impl sidequest_game::tts_stream::TtsSynthesizer for DaemonSynthesizer {
    fn synthesize(
        &self,
        text: &str,
        _speaker: &str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<u8>, sidequest_game::tts_stream::TtsError>>
                + Send
                + '_,
        >,
    > {
        let text = text.to_string();
        Box::pin(async move {
            let params = sidequest_daemon_client::TtsParams {
                text,
                model: "kokoro".to_string(),
                voice_id: "en_male_deep".to_string(),
                speed: 0.95,
                ..Default::default()
            };
            let mut client = self.client.lock().await;
            match client.synthesize(params).await {
                Ok(result) => Ok(result.audio_bytes),
                Err(e) => Err(sidequest_game::tts_stream::TtsError::SynthesisFailed(
                    e.to_string(),
                )),
            }
        })
    }
}



/// Convert a game-internal AudioCue to a protocol GameMessage for WebSocket broadcast.
///
/// `genre_slug` is prepended to track paths so the client can fetch via `/genre/{slug}/{path}`.
/// `mood` is included so the client's deduplication logic works (it ignores cues without mood).
fn audio_cue_to_game_message(
    cue: &sidequest_game::AudioCue,
    player_id: &str,
    genre_slug: &str,
    mood: Option<&str>,
) -> GameMessage {
    let full_track = cue.track_id.as_ref().map(|path| {
        if genre_slug.is_empty() {
            path.clone()
        } else {
            format!("/genre/{}/{}", genre_slug, path)
        }
    });
    GameMessage::AudioCue {
        payload: AudioCuePayload {
            mood: mood.map(|s| s.to_string()),
            music_track: full_track,
            sfx_triggers: vec![],
            channel: Some(cue.channel.to_string()),
            action: Some(cue.action.to_string()),
            volume: Some(cue.volume),
        },
        player_id: player_id.to_string(),
    }
}


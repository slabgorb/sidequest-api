//! Session connect and character creation dispatch.
//!
//! Handles SESSION_EVENT{connect} (new + returning players) and
//! CHARACTER_CREATION messages (chargen scene choices + confirmation).

use std::collections::HashMap;
use std::sync::Arc;

use sidequest_game::builder::CharacterBuilder;
use sidequest_genre::GenreCode;
use sidequest_protocol::{
    ChapterMarkerPayload, CharacterCreationPayload, CharacterSheetPayload, CharacterState,
    GameMessage, InitialState, NarrationEndPayload, NarrationPayload, PartyMember,
    PartyStatusPayload, SessionEventPayload,
};

use crate::npc_context;
use crate::session::Session;
use crate::{error_response, AppState, NpcRegistryEntry, WatcherEventBuilder, WatcherEventType};
use crate::shared_session;

pub(crate) async fn dispatch_connect(
    payload: &SessionEventPayload,
    session: &mut Session,
    builder: &mut Option<CharacterBuilder>,
    player_name_store: &mut Option<String>,
    character_json_store: &mut Option<serde_json::Value>,
    character_name_store: &mut Option<String>,
    character_hp: &mut i32,
    character_max_hp: &mut i32,
    current_location: &mut String,
    discovered_regions: &mut Vec<String>,
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
    turn_manager: &mut sidequest_game::TurnManager,
    npc_registry: &mut Vec<NpcRegistryEntry>,
    lore_store: &mut sidequest_game::LoreStore,
    opening_seed: &mut Option<String>,
    opening_directive: &mut Option<String>,
    state: &AppState,
    player_id: &str,
    _continuity_corrections: &mut String,
    inventory: &mut sidequest_game::Inventory,
    snapshot: &mut sidequest_game::state::GameSnapshot,
    _tx: &tokio::sync::mpsc::Sender<sidequest_protocol::GameMessage>,
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

                        // Extract character data from saved snapshot
                        if let Some(character) = saved.snapshot.characters.first() {
                            *character_json_store =
                                Some(serde_json::to_value(character).unwrap_or_default());
                            *character_name_store = Some(character.core.name.as_str().to_string());
                            *character_hp = character.core.hp;
                            *character_max_hp = character.core.max_hp;
                            *inventory = character.core.inventory.clone();
                        }
                        // Restore location, regions, turn state, and NPC registry from snapshot
                        *current_location = saved.snapshot.location.clone();
                        *discovered_regions = saved.snapshot.discovered_regions.clone();
                        *turn_manager = saved.snapshot.turn_manager.clone();
                        *npc_registry = saved.snapshot.npc_registry.clone();
                        *axis_values = saved.snapshot.axis_values.clone();
                        // Restore canonical snapshot for dispatch pipeline (story 15-8)
                        *snapshot = saved.snapshot.clone();

                        // Transition session to Playing
                        if let Err(e) = session.complete_character_creation() {
                            tracing::error!(error = %e, state = %session.state_name(), "Failed to transition session to Playing on reconnect");
                            return vec![error_response(player_id, &format!("Session transition failed: {e}"))];
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
                                    turn_count: saved.snapshot.turn_manager.round(),
                                }),
                                css: None,
                                image_cooldown_seconds: None,
                                narrator_verbosity: None,
                                narrator_vocabulary: None,
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
                                    current_location: current_location.clone(),
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
                                    current_location: current_location.clone(),
                                })
                                .collect();
                            responses.push(GameMessage::PartyStatus {
                                payload: PartyStatusPayload { members },
                                player_id: player_id.to_string(),
                            });
                        }

                        // Initialize audio subsystems for returning player
                        if let Ok(genre_code) = GenreCode::new(genre) {
                            if let Ok(pack) = state.genre_cache().get_or_load(&genre_code, state.genre_loader()) {
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

                                // Story 15-24: Restore persisted lore fragments from SQLite.
                                match state.persistence().load_lore_fragments(genre, world, pname).await {
                                    Ok(fragments) => {
                                        let restored_count = fragments.len();
                                        for fragment in fragments {
                                            let _ = lore_store.add(fragment);
                                        }
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
                                            WatcherEventBuilder::new("world_materialization", WatcherEventType::StateTransition)
                                                .field("event", "world_materialized")
                                                .field("genre", genre)
                                                .field("world", world)
                                                .field("chapters_available", chapters.len())
                                                .field("chapters_applied", snapshot.world_history.len())
                                                .field("prev_maturity", format!("{:?}", prev_maturity))
                                                .field("new_maturity", format!("{:?}", snapshot.campaign_maturity))
                                                .field("trigger", "returning_player_reconnect")
                                                .send(state);
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
                            opening_seed,
                            opening_directive,
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
                            opening_seed,
                            opening_directive,
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
                    opening_seed,
                    opening_directive,
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
    builder: &mut Option<CharacterBuilder>,
    trope_defs_out: &mut Vec<sidequest_genre::TropeDefinition>,
    world_context_out: &mut String,
    visual_style_out: &mut Option<sidequest_genre::VisualStyle>,
    axes_config_out: &mut Option<sidequest_genre::AxesConfig>,
    music_director_out: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer_lock: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_lock: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>>,
    lore_store: &mut sidequest_game::LoreStore,
    opening_seed_out: &mut Option<String>,
    opening_directive_out: &mut Option<String>,
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

    let pack = match state.genre_cache().get_or_load(&genre_code, state.genre_loader()) {
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
    let culture_ref = npc_context::build_culture_reference(cultures);
    if !culture_ref.is_empty() {
        world_context_out.push_str(&culture_ref);
    }

    // Select a random opening hook if the genre pack provides them
    if !pack.openings.is_empty() {
        use rand::Rng;
        let idx = rand::thread_rng().gen_range(0..pack.openings.len());
        let hook = &pack.openings[idx];

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
            genre = %genre,
            hook_id = %hook.id,
            archetype = %hook.archetype,
            "opening_hook_selected"
        );
    }

    // Filter scenes to those with non-empty choices
    let scenes: Vec<_> = pack
        .char_creation
        .iter()
        .filter(|s| !s.choices.is_empty())
        .cloned()
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
pub(crate) async fn dispatch_character_creation(
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
    opening_seed: &Option<String>,
    opening_directive: &mut Option<String>,
    axes_config: &Option<sidequest_genre::AxesConfig>,
    axis_values: &mut Vec<sidequest_game::axis::AxisValue>,
    visual_style: &Option<sidequest_genre::VisualStyle>,
    npc_registry: &mut Vec<NpcRegistryEntry>,
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
    quest_log: &mut HashMap<String, String>,
    genie_wishes: &mut Vec<sidequest_game::GenieWish>,
    resource_state: &mut HashMap<String, f64>,
    resource_declarations: &[sidequest_genre::ResourceDeclaration],
    achievement_tracker: &mut sidequest_game::achievement::AchievementTracker,
    snapshot: &mut sidequest_game::state::GameSnapshot,
    narrator_verbosity: sidequest_protocol::NarratorVerbosity,
    narrator_vocabulary: sidequest_protocol::NarratorVocabulary,
    pending_trope_context: &mut Option<String>,
    tx: &tokio::sync::mpsc::Sender<sidequest_protocol::GameMessage>,
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

            WatcherEventBuilder::new("character_creation", WatcherEventType::StateTransition)
                .field("phase", phase)
                .field("choice_index", index)
                .field("player_id", player_id)
                .send(state);

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

                    WatcherEventBuilder::new("character_creation", WatcherEventType::StateTransition)
                        .field("event", "character_built")
                        .field("name", character.core.name.as_str())
                        .field("class", character.char_class.as_str())
                        .field("race", character.race.as_str())
                        .field("hp", character.core.hp)
                        .send(state);

                    // Store character data — sync ALL mutable fields from the built character
                    *character_name_store = Some(character.core.name.as_str().to_string());
                    *character_hp = character.core.hp;
                    *character_max_hp = character.core.max_hp;
                    *inventory = character.core.inventory.clone();

                    // Wire starting equipment from genre pack's inventory.yaml.
                    // The data exists, the parser exists, sidequest-loadoutgen reads
                    // it — but chargen never called any of it.  Classic wiring gap.
                    {
                        let char_class = character.char_class.as_str().to_string();
                        let genre_slug = session.genre_slug().unwrap_or("").to_string();
                        if let Ok(gc) = GenreCode::new(&genre_slug) {
                            if let Ok(pack) = state.genre_cache().get_or_load(&gc, state.genre_loader()) {
                                if let Some(ref inv_config) = pack.inventory {
                                    // Match class name case-insensitively
                                    let class_lower = char_class.to_lowercase();
                                    let equipment_ids: Vec<String> = inv_config.starting_equipment.iter()
                                        .find(|(k, _)| k.to_lowercase() == class_lower)
                                        .map(|(_, v)| v.clone())
                                        .unwrap_or_default();
                                    let gold = inv_config.starting_gold.iter()
                                        .find(|(k, _)| k.to_lowercase() == class_lower)
                                        .map(|(_, v)| *v)
                                        .unwrap_or(0);

                                    // Resolve item IDs from catalog
                                    for item_id in &equipment_ids {
                                        if let Some(catalog_item) = inv_config.item_catalog.iter().find(|ci| ci.id == *item_id) {
                                            let rarity_str = if catalog_item.rarity.is_empty() { "common" } else { &catalog_item.rarity };
                                            if let (Ok(name), Ok(desc), Ok(cat), Ok(rarity)) = (
                                                sidequest_protocol::NonBlankString::new(&catalog_item.name),
                                                sidequest_protocol::NonBlankString::new(&catalog_item.description),
                                                sidequest_protocol::NonBlankString::new(&catalog_item.category),
                                                sidequest_protocol::NonBlankString::new(rarity_str),
                                            ) {
                                                inventory.items.push(sidequest_game::Item {
                                                    id: sidequest_protocol::NonBlankString::new(&catalog_item.id).unwrap_or(name.clone()),
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
                                            if let (Ok(id_nb), Ok(name_nb), Ok(desc_nb), Ok(cat_nb), Ok(rar_nb)) = (
                                                sidequest_protocol::NonBlankString::new(item_id),
                                                sidequest_protocol::NonBlankString::new(&display),
                                                sidequest_protocol::NonBlankString::new("Starting equipment"),
                                                sidequest_protocol::NonBlankString::new("equipment"),
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
                        *character_json_store = Some(serde_json::to_value(&updated_char).unwrap_or_default());
                    }
                    tracing::info!(
                        char_name = %character.core.name,
                        hp = character.core.hp,
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
                            .and_then(|gc| state.genre_cache().get_or_load(&gc, state.genre_loader()).ok())
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
                        WatcherEventBuilder::new("world_materialization", WatcherEventType::StateTransition)
                            .field("event", "world_materialized")
                            .field("genre", genre.as_str())
                            .field("world", world.as_str())
                            .field("chapters_applied", snap.world_history.len())
                            .field("maturity", format!("{:?}", snap.campaign_maturity))
                            .field("trigger", "new_player_chargen")
                            .send(state);

                        // Inject the chargen-produced character into the materialized snapshot
                        snap.characters = vec![character.clone()];
                        // Sync post-loadout inventory into snapshot character.
                        // The `character` object has only builder item_hints; the full
                        // loadout from inventory.yaml was added to the `inventory` local.
                        if let Some(ch) = snap.characters.first_mut() {
                            ch.core.inventory = inventory.clone();
                        }

                        // Room-graph mode: set starting location to entrance room (story 19-2)
                        let rooms_for_init: Vec<sidequest_genre::RoomDef> = match GenreCode::new(&genre) {
                            Ok(gc) => match state.genre_cache().get_or_load(&gc, state.genre_loader()) {
                                Ok(pack) => pack.worlds.get(&world).cloned()
                                    .filter(|w| w.cartography.navigation_mode == sidequest_genre::NavigationMode::RoomGraph)
                                    .and_then(|w| w.cartography.rooms.clone())
                                    .unwrap_or_default(),
                                Err(e) => {
                                    tracing::warn!(error = %e, genre = %genre, world = %world, "Failed to load genre pack for room-graph init");
                                    vec![]
                                }
                            },
                            Err(e) => {
                                tracing::warn!(error = %e, genre = %genre, "Invalid genre code for room-graph init");
                                vec![]
                            }
                        };
                        if !rooms_for_init.is_empty() {
                            sidequest_game::room_movement::init_room_graph_location(&mut snap, &rooms_for_init);
                            tracing::info!(
                                location = %snap.location,
                                discovered_rooms = snap.discovered_rooms.len(),
                                "room_graph.init — entrance room set"
                            );
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
                            .and_then(|gc| state.genre_cache().get_or_load(&gc, state.genre_loader()).ok())
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
                        }
                    }
                    // Seed discovered_regions from snapshot location
                    if !current_location.is_empty() && !discovered_regions.iter().any(|r| r == current_location.as_str()) {
                        discovered_regions.push(current_location.clone());
                    }

                    if let Err(e) = state
                        .persistence()
                        .save(&genre, &world, &pname_for_save, &snapshot)
                        .await
                    {
                        tracing::warn!(error = %e, genre = %genre, world = %world, player = %pname_for_save, "Failed to persist initial session");
                    }

                    // Transition session to Playing
                    if let Err(e) = session.complete_character_creation() {
                        tracing::error!(error = %e, state = %session.state_name(), "Failed to transition session to Playing after chargen");
                        return vec![error_response(player_id, &format!("Session transition failed: {e}"))];
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
                            choice: None,
                            character: Some(char_json),
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
                                    name: character.core.name.as_str().to_string(),
                                    hp: *character_hp,
                                    max_hp: *character_max_hp,
                                    level: *character_level as u32,
                                    class: character.char_class.as_str().to_string(),
                                    statuses: vec![],
                                    inventory: inventory.carried()
                                        .map(|i| i.name.as_str().to_string())
                                        .collect(),
                                }],
                                location: current_location.clone(),
                                quests: quest_log.clone(),
                                turn_count: turn_manager.interaction() as u32,
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
                        let mut monster_manual = sidequest_game::monster_manual::MonsterManual::load(gs_mm, ws_mm);
                        if monster_manual.needs_seeding() && !gs_mm.is_empty() {
                            super::pregen::seed_manual(state, gs_mm, &mut monster_manual);
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
                            hp: character_hp,
                            max_hp: character_max_hp,
                            level: character_level,
                            xp: character_xp,
                            current_location,
                            inventory,
                            character_json: character_json_store,
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
                            continuity_corrections,
                            genie_wishes,
                            resource_state,
                            resource_declarations,
                            sfx_library: {
                                let genre_slug = session.genre_slug().unwrap_or("");
                                sidequest_genre::GenreCode::new(genre_slug)
                                    .ok()
                                    .and_then(|gc| state.genre_cache().get_or_load(&gc, state.genre_loader()).ok())
                                    .map(|pack| pack.audio.sfx_library.clone())
                                    .unwrap_or_default()
                            },
                            rooms: {
                                let gs = session.genre_slug().unwrap_or("");
                                let ws = session.world_slug().unwrap_or("");
                                match sidequest_genre::GenreCode::new(gs) {
                                    Ok(gc) => match state.genre_cache().get_or_load(&gc, state.genre_loader()) {
                                        Ok(pack) => pack.worlds.get(ws).cloned()
                                            .filter(|world| world.cartography.navigation_mode == sidequest_genre::NavigationMode::RoomGraph)
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
                                    .and_then(|gc| state.genre_cache().get_or_load(&gc, state.genre_loader()).ok())
                                    .map(|pack| pack.progression.affinities.clone())
                                    .unwrap_or_default()
                            },
                            world_graph: {
                                let gs = session.genre_slug().unwrap_or("");
                                let ws = session.world_slug().unwrap_or("");
                                sidequest_genre::GenreCode::new(gs)
                                    .ok()
                                    .and_then(|gc| state.genre_cache().get_or_load(&gc, state.genre_loader()).ok())
                                    .and_then(|pack| pack.worlds.get(ws).cloned())
                                    .and_then(|world| world.cartography.world_graph)
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
                        };
                        let result = super::dispatch_player_action(&mut ctx).await;
                        ctx.monster_manual.save();
                        result
                    };

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
                            equipment: inventory.carried().map(|i| {
                                if i.equipped {
                                    format!("{} [equipped]", i.name)
                                } else {
                                    i.name.as_str().to_string()
                                }
                            }).collect(),
                            portrait_url: None,
                            current_location: current_location.clone(),
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
                                p.inventory = inventory.clone();
                                p.character_xp = character.core.xp;
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
                            // Build and send targeted PARTY_STATUS to OTHER session members.
                            // The current player gets their PartyStatus from the opening
                            // narration dispatch — sending here too causes duplicates.
                            // Skip entirely for single-player (no other members to notify).
                            if ss.players.len() <= 1 {
                                tracing::debug!("Skipping connect PartyStatus — single player, dispatch will send");
                            } else {
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
                                            current_location: current_location.clone(),
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
                                            current_location: ps.display_location.clone(),
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
                            } // end multiplayer PartyStatus
                            let pc = ss.player_count();
                            tracing::info!(
                                player_id = %player_id,
                                player_count = pc,
                                "Player joined shared session"
                            );
                            WatcherEventBuilder::new("multiplayer", WatcherEventType::StateTransition)
                                .field("event", "session_joined")
                                .field("session_key", format!("{}:{}", genre, world))
                                .field("player_count", pc)
                                .send(state);

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

                    // "ready" must come AFTER intro_messages.  The auto-turn
                    // ("I look around") sends its NARRATION inline via ctx.tx
                    // inside dispatch_player_action, so by the time we reach
                    // here the narration is already in the mpsc queue.  The
                    // client's "ready" handler clears the narration buffer, so
                    // if "ready" arrives before the narration flushes the
                    // opening text is wiped.  Placing "ready" last ensures all
                    // turn-1 messages are delivered first.
                    let mut msgs = vec![
                        complete,
                        char_sheet,
                        backstory_narration,
                        backstory_end,
                    ];
                    msgs.extend(intro_messages);
                    msgs.push(ready);
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

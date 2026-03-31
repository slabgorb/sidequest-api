//! Player action dispatch — the main game loop handler.
//!
//! Extracted from lib.rs for maintainability. This module contains
//! `dispatch_player_action`, the monolithic handler that orchestrates
//! narration, combat, chase, tropes, renders, music, and state updates.

use std::collections::HashMap;
use std::sync::Arc;

use tracing::Instrument;

use sidequest_agents::orchestrator::TurnContext;
use sidequest_genre::{GenreCode, GenreLoader};
use sidequest_protocol::{
    ActionRevealPayload, ChapterMarkerPayload, CombatEventPayload, GameMessage, InventoryPayload,
    MapUpdatePayload, NarrationEndPayload, NarrationPayload, PartyMember, PartyStatusPayload,
    PlayerActionEntry, SessionEventPayload, ThinkingPayload, TurnStatusPayload,
};

use crate::extraction::{
    audio_cue_to_game_message, extract_location_header, strip_location_header,
    strip_markdown_for_tts,
};
use crate::npc_context::build_npc_registry_context;
use crate::{
    shared_session, AppState, DaemonSynthesizer, NpcRegistryEntry, Severity, WatcherEvent,
    WatcherEventType,
};

/// Mutable per-player state passed through the dispatch pipeline.
pub(crate) struct DispatchContext<'a> {
    pub action: &'a str,
    pub aside: bool,
    pub char_name: &'a str,
    pub player_id: &'a str,
    pub genre_slug: &'a str,
    pub world_slug: &'a str,
    pub player_name_for_save: &'a str,
    pub hp: &'a mut i32,
    pub max_hp: &'a mut i32,
    pub level: &'a mut u32,
    pub xp: &'a mut u32,
    pub current_location: &'a mut String,
    pub inventory: &'a mut sidequest_game::Inventory,
    pub character_json: &'a mut Option<serde_json::Value>,
    pub combat_state: &'a mut sidequest_game::combat::CombatState,
    pub chase_state: &'a mut Option<sidequest_game::ChaseState>,
    pub trope_states: &'a mut Vec<sidequest_game::trope::TropeState>,
    pub trope_defs: &'a [sidequest_genre::TropeDefinition],
    pub world_context: &'a str,
    pub axes_config: &'a Option<sidequest_genre::AxesConfig>,
    pub axis_values: &'a mut Vec<sidequest_game::axis::AxisValue>,
    pub visual_style: &'a Option<sidequest_genre::VisualStyle>,
    pub npc_registry: &'a mut Vec<NpcRegistryEntry>,
    pub quest_log: &'a mut HashMap<String, String>,
    pub narration_history: &'a mut Vec<String>,
    pub discovered_regions: &'a mut Vec<String>,
    pub turn_manager: &'a mut sidequest_game::TurnManager,
    pub lore_store: &'a sidequest_game::LoreStore,
    pub shared_session_holder: &'a Arc<
        tokio::sync::Mutex<Option<Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>,
    >,
    pub music_director: &'a mut Option<sidequest_game::MusicDirector>,
    pub audio_mixer: &'a Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    pub prerender_scheduler:
        &'a Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>>,
    pub state: &'a AppState,
    pub continuity_corrections: &'a mut String,
    pub genie_wishes: &'a mut Vec<sidequest_game::GenieWish>,
    pub resource_state: &'a mut HashMap<String, f64>,
    pub resource_declarations: &'a [sidequest_genre::ResourceDeclaration],
    pub narrator_verbosity: sidequest_protocol::NarratorVerbosity,
    pub narrator_vocabulary: sidequest_protocol::NarratorVocabulary,
}

/// Handle PLAYER_ACTION — send THINKING, narration, NARRATION_END, PARTY_STATUS.
pub(crate) async fn dispatch_player_action(ctx: &mut DispatchContext<'_>) -> Vec<GameMessage> {
    let turn_span = tracing::info_span!(
        "turn",
        ctx.player_id = %ctx.player_id,
        ctx.action = %&ctx.action[..ctx.action.len().min(80)],
        turn_number = tracing::field::Empty,
        agent = tracing::field::Empty,
        intent = tracing::field::Empty,
    );
    let _turn_guard = turn_span.enter();

    // Sync world-level state from shared session (if multiplayer)
    {
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            ss.sync_to_locals(
                ctx.current_location,
                ctx.npc_registry,
                ctx.narration_history,
                ctx.discovered_regions,
                ctx.trope_states,
            );
            // Sync per-player state from barrier modifications (HP, inventory, combat, etc.)
            ss.sync_player_to_locals(
                ctx.player_id,
                ctx.hp,
                ctx.max_hp,
                ctx.level,
                ctx.xp,
                ctx.inventory,
                ctx.combat_state,
                ctx.chase_state,
                ctx.character_json,
            );
            let pc = ss.player_count();
            if pc > 1 {
                ctx.state.send_watcher_event(WatcherEvent {
                    timestamp: chrono::Utc::now(),
                    component: "multiplayer".to_string(),
                    event_type: WatcherEventType::AgentSpanOpen,
                    severity: Severity::Info,
                    fields: {
                        let mut f = HashMap::new();
                        f.insert("event".to_string(), serde_json::json!("multiplayer_action"));
                        f.insert(
                            "session_key".to_string(),
                            serde_json::json!(format!("{}:{}", ctx.genre_slug, ctx.world_slug)),
                        );
                        f.insert("player_id".to_string(), serde_json::json!(ctx.player_id));
                        f.insert("party_size".to_string(), serde_json::json!(pc));
                        f
                    },
                });
            }
        }
    }

    // Timing capture for OTEL flame chart spans
    let turn_start = std::time::Instant::now();

    // Watcher: action received
    let turn_number = ctx.turn_manager.interaction();
    turn_span.record("turn_number", turn_number);
    ctx.state.send_watcher_event(WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "game".to_string(),
        event_type: WatcherEventType::AgentSpanOpen,
        severity: Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert(
                "action".to_string(),
                serde_json::Value::String(ctx.action.to_string()),
            );
            f.insert(
                "player".to_string(),
                serde_json::Value::String(ctx.char_name.to_string()),
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
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            if ss.players.len() > 1 {
                let turn_active = GameMessage::TurnStatus {
                    payload: TurnStatusPayload {
                        player_name: ctx.player_name_for_save.to_string(),
                        status: "active".into(),
                        state_delta: None,
                    },
                    player_id: ctx.player_id.to_string(),
                };
                let _ = ctx.state.broadcast(turn_active);
                tracing::info!(player_id = %ctx.player_id, player_name = %ctx.player_name_for_save, "turn_status.active broadcast to all clients");
            }
        }
    }

    // THINKING indicator — send eagerly BEFORE LLM call so UI shows it immediately.
    // Send only to the acting player via session channel (not global broadcast)
    // so that other players' input is not blocked by the "narrator thinking" lock.
    let thinking = GameMessage::Thinking {
        payload: ThinkingPayload {},
        player_id: ctx.player_id.to_string(),
    };
    tracing::info!(player_id = %ctx.player_id, "thinking.sent");
    {
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            ss.send_to_player(thinking.clone(), ctx.player_id.to_string());
        } else {
            // Single-player fallback: use global broadcast
            let _ = ctx.state.broadcast(thinking.clone());
        }
    }

    // Slash command interception — route /commands to mechanical handlers, not the LLM.
    if let Some(slash_messages) = handle_slash_command(ctx) {
        return slash_messages;
    }

    // Aside handling — narrate with flavor but skip ALL state mutations.
    // Asides are out-of-character commentary that should not affect the game world.
    if ctx.aside {
        return handle_aside(ctx).await;
    }

    let mut state_summary = build_prompt_context(ctx).await;

    // Check if barrier mode is active (Structured/Cinematic turn mode).
    // If active, submit action to barrier, send "waiting" to this player via session
    // channel, then await barrier resolution inline. After resolution, override the
    // action with the combined party context and fall through to the normal pipeline.
    // This ensures ALL post-narration systems (HP, combat, tropes, quests, inventory,
    // persistence, music, render, etc.) run for barrier turns — not just narration.
    let barrier_combined_action: Option<String> = {
        let holder = ctx.shared_session_holder.lock().await;
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
                    tracing::info!(player_id = %ctx.player_id, "barrier.submit — action submitted, waiting for other players");
                    barrier.submit_action(ctx.player_id, ctx.action);

                    // Broadcast TURN_STATUS "active" so other players' UIs know this player submitted
                    let turn_submitted = GameMessage::TurnStatus {
                        payload: TurnStatusPayload {
                            player_name: ctx.player_name_for_save.to_string(),
                            status: "active".into(),
                            state_delta: None,
                        },
                        player_id: ctx.player_id.to_string(),
                    };
                    let _ = ctx.state.broadcast(turn_submitted);
                    tracing::info!(player_name = %ctx.player_name_for_save, "barrier.turn_status.active — broadcast submission notification");
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
                                narrator_verbosity: None,
                                narrator_vocabulary: None,
                                image_cooldown_seconds: None,
                            },
                            player_id: ctx.player_id.to_string(),
                        },
                        ctx.player_id.to_string(),
                    );

                    drop(ss);
                    drop(holder);

                    // Await barrier resolution inline — player is waiting, handler blocked is fine
                    let result = barrier_clone.wait_for_turn().await;
                    tracing::info!(
                        timed_out = result.timed_out,
                        missing = ?result.missing_players,
                        genre = %ctx.genre_slug,
                        world = %ctx.world_slug,
                        "Turn barrier resolved"
                    );

                    // Extract auto-resolved info before dropping result
                    let auto_resolved_names = result.auto_resolved_character_names();
                    let auto_resolved_context = result.format_auto_resolved_context();

                    // Build combined action context from all players' submissions
                    let named_actions = {
                        let holder = ctx.shared_session_holder.lock().await;
                        if let Some(ref ss_arc) = *holder {
                            let ss = ss_arc.lock().await;
                            ss.multiplayer.named_actions()
                        } else {
                            HashMap::new()
                        }
                    };
                    let combined = named_actions
                        .iter()
                        .map(|(name, act)| format!("{}: {}", name, act))
                        .collect::<Vec<_>>()
                        .join("\n");

                    // Broadcast ACTION_REVEAL with auto_resolved field populated
                    let turn_number = barrier_clone.turn_number().saturating_sub(1);
                    let action_entries: Vec<PlayerActionEntry> = named_actions
                        .iter()
                        .map(|(name, _action)| PlayerActionEntry {
                            character_name: name.clone(),
                            player_id: String::new(),
                            action: ctx.action.to_string(),
                        })
                        .collect();
                    let reveal = GameMessage::ActionReveal {
                        payload: ActionRevealPayload {
                            actions: action_entries,
                            turn_number,
                            auto_resolved: auto_resolved_names.clone(),
                        },
                        player_id: "server".to_string(),
                    };
                    let _ = ctx.state.broadcast(reveal);
                    tracing::info!(auto_resolved = ?auto_resolved_names, "barrier.action_reveal — broadcast with auto-resolved");

                    // Broadcast TURN_STATUS "auto_resolved" for each timed-out player
                    for name in &auto_resolved_names {
                        let turn_auto = GameMessage::TurnStatus {
                            payload: TurnStatusPayload {
                                player_name: name.clone(),
                                status: "auto_resolved".into(),
                                state_delta: None,
                            },
                            player_id: "server".to_string(),
                        };
                        let _ = ctx.state.broadcast(turn_auto);
                    }

                    // Prepend combined actions + auto-resolved context + perspective instruction
                    let auto_ctx = if auto_resolved_context.is_empty() {
                        String::new()
                    } else {
                        format!("\n{}\n", auto_resolved_context)
                    };
                    state_summary = format!(
                        "Combined party actions:\n{}\n{}\nPERSPECTIVE: Write in third-person omniscient. Do NOT use 'you' for any character. Name all characters explicitly.\n\n{}",
                        combined, auto_ctx, state_summary
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
        None => std::borrow::Cow::Borrowed(ctx.action),
    };

    // Preprocess raw player input — STT cleanup + three-perspective rewrite.
    // Uses haiku-tier LLM with 15s timeout; falls back to mechanical rewrite on failure.
    let preprocess_span = tracing::info_span!("turn.preprocess", raw_len = ctx.action.len());
    let _preprocess_guard = preprocess_span.enter();
    let preprocessed =
        sidequest_agents::preprocessor::preprocess_action(&effective_action, ctx.char_name);
    tracing::info!(
        raw = %ctx.action,
        you = %preprocessed.you,
        named = %preprocessed.named,
        intent = %preprocessed.intent,
        "Action preprocessed"
    );

    // F9: Wish Consequence Engine — LLM-classified power-grab on clean input.
    if preprocessed.is_power_grab {
        let mut engine =
            sidequest_game::WishConsequenceEngine::with_counter(ctx.genie_wishes.len());
        if let Some(wish) = engine.evaluate(ctx.char_name, &preprocessed.intent, true) {
            let wish_context = sidequest_game::WishConsequenceEngine::build_prompt_context(&wish);
            tracing::info!(
                wisher = %wish.wisher_name,
                category = ?wish.consequence_category,
                rotation = ctx.genie_wishes.len(),
                "wish_consequence.power_grab_detected"
            );
            state_summary.push_str(&wish_context);
            ctx.genie_wishes.push(wish);
        }
    }

    drop(_preprocess_guard);
    let preprocess_done = std::time::Instant::now();

    // Process the action through GameService (FreePlay mode — immediate resolution)
    let context = TurnContext {
        state_summary: Some(state_summary),
        in_combat: ctx.combat_state.in_combat(),
        in_chase: ctx.chase_state.is_some(),
        narrator_verbosity: ctx.narrator_verbosity,
        narrator_vocabulary: ctx.narrator_vocabulary,
    };
    let result = ctx
        .state
        .game_service()
        .process_action(&preprocessed.you, &context);

    if let Some(ref intent) = result.classified_intent {
        turn_span.record("intent", intent.as_str());
    }
    if let Some(ref agent) = result.agent_name {
        turn_span.record("agent", agent.as_str());
    }

    // Watcher: narration generated (with intent classification and agent routing)
    ctx.state.send_watcher_event(WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "agent".to_string(),
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
            if let Some(dur) = result.agent_duration_ms {
                f.insert("agent_duration_ms".to_string(), serde_json::json!(dur));
            }
            if let Some(t) = result.token_count_in {
                f.insert("token_count_in".to_string(), serde_json::json!(t));
            }
            if let Some(t) = result.token_count_out {
                f.insert("token_count_out".to_string(), serde_json::json!(t));
            }
            if let Some(tier) = result.extraction_tier {
                f.insert("extraction_tier".to_string(), serde_json::json!(tier));
            }
            f
        },
    });

    let agent_done = std::time::Instant::now();

    let mut messages = vec![];

    // Extract location header from narration (format: **Location Name**\n\n...)
    let state_update_span = tracing::info_span!(
        "turn.state_update",
        location_changed = tracing::field::Empty,
        items_gained = tracing::field::Empty,
    );
    let _state_update_guard = state_update_span.enter();

    // Bug 1: Update current_location so subsequent turns maintain continuity
    let narration_text = &result.narration;
    if let Some(location) = extract_location_header(narration_text) {
        let is_new = !ctx.discovered_regions.iter().any(|r| r == &location);
        *ctx.current_location = location.clone();
        if is_new {
            ctx.discovered_regions.push(location.clone());
        }
        tracing::info!(
            location = %location,
            is_new,
            total_discovered = ctx.discovered_regions.len(),
            "location.changed"
        );
        ctx.state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "state".to_string(),
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
            player_id: ctx.player_id.to_string(),
        });
        // Build explored locations from discovered_regions
        let explored_locs: Vec<sidequest_protocol::ExploredLocation> = ctx
            .discovered_regions
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
                region: ctx.current_location.clone(),
                explored: explored_locs,
                fog_bounds: None,
            },
            player_id: ctx.player_id.to_string(),
        });
        // Location change = meaningful narrative beat → advance display round
        ctx.turn_manager.advance_round();
        tracing::info!(
            new_round = ctx.turn_manager.round(),
            interaction = ctx.turn_manager.interaction(),
            "turn_manager.advance_round — location change"
        );
    }

    // Strip the location header from narration text if present
    let clean_narration = strip_location_header(narration_text);

    // Bug 17: Accumulate narration history for context on subsequent turns.
    // Truncate narrator response to ~300 chars to keep context bounded.
    let truncated_narration: String = clean_narration.chars().take(300).collect();
    ctx.narration_history.push(format!(
        "[{}] Action: {}\nNarrator: {}",
        ctx.char_name, effective_action, truncated_narration
    ));
    // Cap the buffer at 20 entries to prevent unbounded growth
    if ctx.narration_history.len() > 20 {
        ctx.narration_history
            .drain(..ctx.narration_history.len() - 20);
    }

    // Update NPC registry from structured narrator output.
    // If the narrator doesn't emit npcs_present, NPCs are missed. That's the narrator's job.
    let turn_approx = ctx.turn_manager.interaction() as u32;
    if !result.npcs_present.is_empty() {
        tracing::info!(
            count = result.npcs_present.len(),
            "npc_registry.structured — updating from narrator JSON"
        );
        for npc in &result.npcs_present {
            if npc.name.is_empty() {
                continue;
            }
            let name_lower = npc.name.to_lowercase();
            if let Some(entry) = ctx.npc_registry.iter_mut().find(|e| {
                e.name.to_lowercase() == name_lower
                    || e.name.to_lowercase().contains(&name_lower)
                    || name_lower.contains(&e.name.to_lowercase())
            }) {
                // Update existing — preserve identity, update last_seen
                entry.last_seen_turn = turn_approx;
                if !ctx.current_location.is_empty() {
                    entry.location = ctx.current_location.to_string();
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
                // ── OTel: OCEAN personality assignment ────────────────────
                let span = tracing::info_span!(
                    "npc.ocean_assignment",
                    npc_name = %npc.name,
                    npc_role = %npc.role,
                    ocean_summary = tracing::field::Empty,
                    archetype_source = tracing::field::Empty,
                    genre = %ctx.genre_slug,
                );
                let _guard = span.enter();

                // New NPC — create entry with OCEAN personality from genre archetype
                let (ocean_profile, source) = {
                    let loader = GenreLoader::new(vec![ctx.state.genre_packs_path().to_path_buf()]);
                    match GenreCode::new(ctx.genre_slug) {
                        Ok(genre_code) => match loader.load(&genre_code) {
                            Ok(pack) => Some(pack),
                            Err(e) => {
                                tracing::warn!(genre = %ctx.genre_slug, error = %e, "Failed to load genre pack for NPC OCEAN profile");
                                None
                            }
                        },
                        Err(e) => {
                            tracing::warn!(genre = %ctx.genre_slug, error = %e, "Invalid genre code for NPC OCEAN profile");
                            None
                        }
                    }
                        .and_then(|pack| {
                            let with_ocean: Vec<_> = pack
                                .archetypes
                                .iter()
                                .filter(|a| a.ocean.is_some())
                                .collect();
                            if with_ocean.is_empty() {
                                return None;
                            }
                            use rand::prelude::IndexedRandom;
                            let archetype = with_ocean.choose(&mut rand::rng())?;
                            let profile = archetype.ocean.as_ref()?.with_jitter(1.5);
                            Some((profile, archetype.name.as_str().to_string()))
                        })
                        .unwrap_or_else(|| {
                            (
                                sidequest_genre::OceanProfile::random(),
                                "random".to_string(),
                            )
                        })
                };
                let ocean_summary = ocean_profile.behavioral_summary();
                span.record("ocean_summary", &ocean_summary.as_str());
                span.record("archetype_source", &source.as_str());
                tracing::info!(
                    name = %npc.name, pronouns = %npc.pronouns, role = %npc.role,
                    ocean = %ocean_summary, archetype = %source,
                    "npc_registry.new — created from structured data with OCEAN personality"
                );
                ctx.npc_registry.push(NpcRegistryEntry {
                    name: npc.name.clone(),
                    pronouns: npc.pronouns.clone(),
                    role: npc.role.clone(),
                    age: String::new(),
                    appearance: npc.appearance.clone(),
                    location: ctx.current_location.to_string(),
                    last_seen_turn: turn_approx,
                    ocean_summary,
                    ocean: Some(ocean_profile),
                    hp: 0,
                    max_hp: 0,
                });
            }
        }
    }
    tracing::debug!(
        npc_count = ctx.npc_registry.len(),
        "NPC registry updated from structured extraction"
    );

    // ── OCEAN personality shifts — use structured extraction from narrator, merge with regex fallback ──
    {
        // Convert narrator's structured personality events to typed enum format
        let mut personality_events: Vec<(String, sidequest_game::PersonalityEvent)> = result
            .personality_events
            .iter()
            .filter_map(|pe| {
                let event_lower = pe.event.to_lowercase();
                let typed = if event_lower.contains("betray") || event_lower.contains("treachery") || event_lower.contains("backstab") {
                    Some(sidequest_game::PersonalityEvent::Betrayal)
                } else if event_lower.contains("near death") || event_lower.contains("near-death") || event_lower.contains("nearly died") || event_lower.contains("mortally") {
                    Some(sidequest_game::PersonalityEvent::NearDeath)
                } else if event_lower.contains("victory") || event_lower.contains("triumph") || event_lower.contains("vanquish") || event_lower.contains("prevail") {
                    Some(sidequest_game::PersonalityEvent::Victory)
                } else if event_lower.contains("defeat") || event_lower.contains("vanquished") || event_lower.contains("overwhelmed") || event_lower.contains("routed") {
                    Some(sidequest_game::PersonalityEvent::Defeat)
                } else if event_lower.contains("bond") || event_lower.contains("friendship") || event_lower.contains("trust") || event_lower.contains("connection") {
                    Some(sidequest_game::PersonalityEvent::SocialBonding)
                } else {
                    None
                };
                typed.map(|t| (pe.npc.clone(), t))
            })
            .collect();

        // Merge with regex fallback — catches events the structured extraction may have missed
        let npc_names: Vec<&str> = ctx.npc_registry.iter().map(|e| e.name.as_str()).collect();
        let regex_events = sidequest_game::detect_personality_events(&clean_narration, &npc_names);
        for (npc, event) in regex_events {
            if !personality_events.iter().any(|(n, _)| *n == npc) {
                personality_events.push((npc, event));
            }
        }

        if !personality_events.is_empty() {
            let (applied, shift_log) = sidequest_game::apply_ocean_shifts(
                ctx.npc_registry,
                &personality_events,
                turn_approx,
            );
            if !applied.is_empty() {
                tracing::info!(
                    events = personality_events.len(),
                    shifts_applied = applied.len(),
                    shift_log_entries = shift_log.shifts().len(),
                    "ocean_shift.applied — NPC personalities evolved from narrative events"
                );
                for proposal in &applied {
                    tracing::debug!(
                        npc = %proposal.npc_name,
                        dimension = ?proposal.dimension,
                        delta = proposal.delta,
                        cause = %proposal.cause,
                        "ocean_shift.detail"
                    );
                }
            }
        }
    }

    // Continuity validation — check narrator output against game state.
    // Build a minimal snapshot from the local session variables for the validator.
    {
        let mut validation_snapshot = sidequest_game::GameSnapshot {
            location: ctx.current_location.clone(),
            ..sidequest_game::GameSnapshot::default()
        };
        // Reconstruct character with inventory for the validator
        if let Some(ref cj) = ctx.character_json {
            if let Ok(mut ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
                ch.core.hp = *ctx.hp;
                ch.core.inventory = ctx.inventory.clone();
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
            *ctx.continuity_corrections = corrections;
        }
    }

    let tier_events = apply_state_mutations(ctx, &result, &clean_narration, &effective_action);

    // Narration — include character state so the UI state mirror picks it up
    let inventory_names: Vec<String> = ctx
        .inventory
        .items
        .iter()
        .map(|i| i.name.as_str().to_string())
        .collect();
    let char_class_name = ctx
        .character_json
        .as_ref()
        .and_then(|cj| cj.get("char_class"))
        .and_then(|c| c.as_str())
        .unwrap_or("Adventurer");

    // Merge narrator footnotes with affinity tier-up events
    let mut footnotes = result.footnotes.clone();
    for event in &tier_events {
        footnotes.push(sidequest_protocol::Footnote {
            marker: None,
            fact_id: None,
            summary: format!(
                "{}'s {} affinity reached tier {} — {}",
                event.character_name,
                event.affinity_name,
                event.new_tier,
                if event.narration_hint.is_empty() { "a new level of mastery" } else { &event.narration_hint },
            ),
            category: sidequest_protocol::FactCategory::Ability,
            is_new: true,
        });
    }

    messages.push(GameMessage::Narration {
        payload: NarrationPayload {
            text: clean_narration.clone(),
            state_delta: Some(sidequest_protocol::StateDelta {
                location: extract_location_header(narration_text),
                characters: Some(vec![sidequest_protocol::CharacterState {
                    name: ctx.char_name.to_string(),
                    hp: *ctx.hp,
                    max_hp: *ctx.max_hp,
                    level: *ctx.level,
                    class: char_class_name.to_string(),
                    statuses: vec![],
                    inventory: inventory_names.clone(),
                }]),
                quests: if ctx.quest_log.is_empty() {
                    None
                } else {
                    Some(ctx.quest_log.clone())
                },
                items_gained: if result.items_gained.is_empty() {
                    None
                } else {
                    Some(result.items_gained.clone())
                },
            }),
            footnotes,
        },
        player_id: ctx.player_id.to_string(),
    });

    // RAG pipeline: convert new footnotes to discovered facts (story 9-11)
    if !result.footnotes.is_empty() {
        let fact_source = if result.classified_intent.as_deref() == Some("Backstory") {
            sidequest_game::known_fact::FactSource::Backstory
        } else {
            sidequest_game::known_fact::FactSource::Discovery
        };
        let discovered = sidequest_agents::footnotes::footnotes_to_discovered_facts(
            &result.footnotes,
            ctx.char_name,
            ctx.turn_manager.interaction(),
            fact_source,
        );
        if !discovered.is_empty() {
            tracing::info!(
                count = discovered.len(),
                character = %ctx.char_name,
                interaction = ctx.turn_manager.interaction(),
                "rag.footnotes_to_discovered_facts"
            );
            // Wire discovered facts into character JSON for persistence
            if let Some(ref mut cj) = ctx.character_json {
                if let Some(facts_arr) = cj.get_mut("known_facts").and_then(|v| v.as_array_mut()) {
                    for df in &discovered {
                        if let Ok(fact_val) = serde_json::to_value(&df.fact) {
                            facts_arr.push(fact_val);
                        }
                    }
                    tracing::info!(
                        new_facts = discovered.len(),
                        total_facts = facts_arr.len(),
                        "rag.discovered_facts_applied_to_character"
                    );
                }
            }
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
        player_id: ctx.player_id.to_string(),
    });

    // Extract character class from JSON for PartyStatus (owned to avoid borrow conflict)
    let char_class: String = ctx
        .character_json
        .as_ref()
        .and_then(|cj| cj.get("char_class"))
        .and_then(|c| c.as_str())
        .unwrap_or("Adventurer")
        .to_string();

    // Party status — build full party from shared session (multiplayer) or local only (single-player)
    {
        let mut party_members = vec![PartyMember {
            player_id: ctx.player_id.to_string(),
            name: ctx.player_name_for_save.to_string(),
            character_name: ctx.char_name.to_string(),
            current_hp: *ctx.hp,
            max_hp: *ctx.max_hp,
            statuses: vec![],
            class: char_class.to_string(),
            level: *ctx.level,
            portrait_url: None,
            current_location: ctx.current_location.clone(),
        }];
        // In multiplayer, include other players from the shared session
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            for (pid, ps) in &ss.players {
                if pid == ctx.player_id {
                    continue; // Already added above with fresh local data
                }
                party_members.push(PartyMember {
                    player_id: pid.clone(),
                    name: ps.player_name.clone(),
                    character_name: ps
                        .character_name
                        .clone()
                        .unwrap_or_else(|| ps.player_name.clone()),
                    current_hp: ps.character_hp,
                    max_hp: ps.character_max_hp,
                    statuses: vec![],
                    class: String::new(),
                    level: ps.character_level,
                    portrait_url: None,
                    current_location: ps.display_location.clone(),
                });
            }
        }
        messages.push(GameMessage::PartyStatus {
            payload: PartyStatusPayload {
                members: party_members,
            },
            player_id: ctx.player_id.to_string(),
        });
    }

    // Bug 5: Inventory — now wired to actual inventory state
    messages.push(GameMessage::Inventory {
        payload: InventoryPayload {
            items: ctx
                .inventory
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
            gold: ctx.inventory.gold,
        },
        player_id: ctx.player_id.to_string(),
    });

    drop(_state_update_guard);

    let system_tick_span = tracing::info_span!(
        "turn.system_tick",
        combat_changed = tracing::field::Empty,
        tropes_fired = tracing::field::Empty,
    );
    let _system_tick_guard = system_tick_span.enter();

    process_combat_and_chase(ctx, &clean_narration, &result, &mut messages).await;

    process_tropes(ctx, &clean_narration, &mut messages);

    drop(_system_tick_guard);

    let media_span = tracing::info_span!(
        "turn.media",
        render_enqueued = tracing::field::Empty,
        audio_cue_sent = tracing::field::Empty,
    );
    let _media_guard = media_span.enter();

    process_render(ctx, &clean_narration, narration_text, &result).await;

    process_audio(ctx, &clean_narration, &mut messages, &result).await;

    // Record this interaction in the turn manager (granular counter for chronology)
    ctx.turn_manager.record_interaction();
    tracing::info!(
        interaction = ctx.turn_manager.interaction(),
        round = ctx.turn_manager.round(),
        "turn_manager.record_interaction"
    );

    drop(_media_guard);

    // Persist updated game state (location, narration log) for reconnection
    if !ctx.genre_slug.is_empty() && !ctx.world_slug.is_empty() {
        let location =
            extract_location_header(narration_text).unwrap_or_else(|| "Starting area".to_string());
        match ctx
            .state
            .persistence()
            .load(ctx.genre_slug, ctx.world_slug, ctx.player_name_for_save)
            .await
        {
            Ok(Some(saved)) => {
                let mut snapshot = saved.snapshot;
                snapshot.location = location;
                // Sync ALL game state to snapshot for persistence
                snapshot.turn_manager = ctx.turn_manager.clone();
                snapshot.npc_registry = ctx.npc_registry.clone();
                snapshot.genie_wishes = ctx.genie_wishes.clone();
                snapshot.axis_values = ctx.axis_values.clone();
                snapshot.combat = ctx.combat_state.clone();
                snapshot.chase = ctx.chase_state.clone();
                snapshot.discovered_regions = ctx.discovered_regions.clone();
                snapshot.active_tropes = ctx.trope_states.clone();
                snapshot.quest_log = ctx.quest_log.clone();
                // Sync character state (HP, XP, level, inventory, known_facts, affinities)
                if let Some(ref cj) = ctx.character_json {
                    if let Ok(ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone())
                    {
                        if let Some(saved_ch) = snapshot.characters.first_mut() {
                            saved_ch.core.hp = *ctx.hp;
                            saved_ch.core.max_hp = *ctx.max_hp;
                            saved_ch.core.level = *ctx.level;
                            saved_ch.core.inventory = ctx.inventory.clone();
                            saved_ch.known_facts = ch.known_facts.clone();
                            saved_ch.affinities = ch.affinities.clone();
                            saved_ch.narrative_state = ch.narrative_state.clone();
                        }
                    }
                }
                // Append narration to log for recap on reconnect
                snapshot.narrative_log.push(sidequest_game::NarrativeEntry {
                    timestamp: 0,
                    round: ctx.turn_manager.interaction() as u32,
                    author: "narrator".to_string(),
                    content: clean_narration.clone(),
                    tags: vec![],
                    encounter_tags: vec![],
                    speaker: None,
                    entry_type: None,
                });
                match ctx
                    .state
                    .persistence()
                    .save(
                        ctx.genre_slug,
                        ctx.world_slug,
                        ctx.player_name_for_save,
                        &snapshot,
                    )
                    .await
                {
                    Ok(_) => tracing::info!(
                        player = %ctx.player_name_for_save,
                        turn = ctx.turn_manager.interaction(),
                        location = %ctx.current_location,
                        ctx.hp = *ctx.hp,
                        items = ctx.inventory.items.len(),
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
    if !clean_narration.is_empty() && !ctx.state.tts_disabled() {
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

            let player_id_for_tts = ctx.player_id.to_string();
            let state_for_tts = ctx.state.clone();
            let ss_holder_for_tts = ctx.shared_session_holder.clone();
            let tts_config = sidequest_game::tts_stream::TtsStreamConfig::default();
            let streamer = sidequest_game::tts_stream::TtsStreamer::new(tts_config);

            // Clone Arcs for the spawned TTS task (mixer ducking + prerender)
            let mixer_for_tts = std::sync::Arc::clone(ctx.audio_mixer);
            let prerender_for_tts = std::sync::Arc::clone(ctx.prerender_scheduler);
            let genre_slug_for_tts = ctx.genre_slug.to_string();
            let tts_segments_for_prerender = tts_segments.clone();
            let prerender_ctx = sidequest_game::PrerenderContext {
                in_combat: ctx.combat_state.in_combat(),
                combatant_names: if ctx.combat_state.in_combat() {
                    result
                        .npcs_present
                        .iter()
                        .map(|npc| npc.name.clone())
                        .collect()
                } else {
                    vec![]
                },
                pending_destination: extract_location_header(narration_text).map(|s| s.to_string()),
                active_dialogue_npc: ctx.npc_registry.last().map(|e| e.name.clone()),
                art_style: match ctx.visual_style {
                    Some(ref vs) => vs.positive_suffix.clone(),
                    None => "oil_painting".to_string(),
                },
                negative_prompt: match ctx.visual_style {
                    Some(ref vs) => vs.negative_prompt.clone(),
                    None => String::new(),
                },
            };

            let tts_span = tracing::info_span!(
                "tts.pipeline",
                segment_count = tts_segments.len(),
                ctx.player_id = %player_id_for_tts,
            );
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
                                        if let Some(ref queue) = state_for_tts.inner.render_queue {
                                            let _ = queue
                                                .enqueue(
                                                    subject,
                                                    &prerender_ctx.art_style,
                                                    "flux-schnell",
                                                    &prerender_ctx.negative_prompt,
                                                    "", // prerender — no narration context
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
            }.instrument(tts_span));
        }
    }

    // GM Panel: emit full game state snapshot after all mutations
    {
        let turn_approx = ctx.turn_manager.interaction() as u32;
        let npc_data: Vec<serde_json::Value> = ctx
            .npc_registry
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
        let inventory_names: Vec<String> = ctx
            .inventory
            .items
            .iter()
            .map(|i| i.name.as_str().to_string())
            .collect();
        let active_tropes: Vec<serde_json::Value> = ctx
            .trope_states
            .iter()
            .map(|ts| {
                serde_json::json!({
                    "id": ts.trope_definition_id(),
                    "progression": ts.progression(),
                    "status": format!("{:?}", ts.status()),
                })
            })
            .collect();
        let snapshot = serde_json::json!({
            "turn_number": turn_approx,
            "location": ctx.current_location.as_str(),
            "hp": *ctx.hp,
            "max_hp": *ctx.max_hp,
            "level": *ctx.level,
            "xp": *ctx.xp,
            "inventory": inventory_names,
            "npc_registry": npc_data,
            "active_tropes": active_tropes,
            "in_combat": ctx.combat_state.in_combat(),
            "player_id": ctx.player_id,
            "character": ctx.char_name,
        });
        ctx.state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "game".to_string(),
            event_type: WatcherEventType::GameStateSnapshot,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert("turn_number".to_string(), serde_json::json!(turn_approx));
                f.insert("snapshot".to_string(), snapshot);
                f
            },
        });
    }

    sync_back_to_shared_session(
        ctx,
        &messages,
        &clean_narration,
        &char_class,
        &effective_action,
    )
    .await;

    // Build timing spans for flame chart visualization
    let state_done = std::time::Instant::now();
    let preprocess_ms = preprocess_done.duration_since(turn_start).as_millis() as u64;
    let agent_ms = result.agent_duration_ms.unwrap_or_else(|| agent_done.duration_since(preprocess_done).as_millis() as u64);
    let agent_start_ms = preprocess_ms;
    let state_start_ms = agent_start_ms + agent_ms;
    let state_ms = state_done.duration_since(agent_done).as_millis() as u64;
    let total_ms = state_done.duration_since(turn_start).as_millis() as u64;

    let spans = serde_json::json!([
        { "name": "preprocessor", "component": "preprocessor", "start_ms": 0, "duration_ms": preprocess_ms },
        { "name": "agent_llm", "component": result.agent_name.as_deref().unwrap_or("narrator"), "start_ms": agent_start_ms, "duration_ms": agent_ms },
        { "name": "state_patch", "component": "state", "start_ms": state_start_ms, "duration_ms": state_ms },
    ]);

    // Emit TurnComplete event for the OTEL dashboard
    ctx.state.send_watcher_event(WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "game".to_string(),
        event_type: WatcherEventType::TurnComplete,
        severity: if result.is_degraded { Severity::Warn } else { Severity::Info },
        fields: {
            let mut f = HashMap::new();
            f.insert("turn_id".to_string(), serde_json::json!(turn_number));
            f.insert("turn_number".to_string(), serde_json::json!(turn_number));
            f.insert("player_input".to_string(), serde_json::json!(effective_action));
            if let Some(ref intent) = result.classified_intent {
                f.insert("classified_intent".to_string(), serde_json::json!(intent));
            }
            if let Some(ref agent) = result.agent_name {
                f.insert("agent_name".to_string(), serde_json::json!(agent));
            }
            f.insert("agent_duration_ms".to_string(), serde_json::json!(agent_ms));
            f.insert("is_degraded".to_string(), serde_json::json!(result.is_degraded));
            f.insert("player_id".to_string(), serde_json::json!(ctx.player_id));
            if let Some(t) = result.token_count_in {
                f.insert("token_count_in".to_string(), serde_json::json!(t));
            }
            if let Some(t) = result.token_count_out {
                f.insert("token_count_out".to_string(), serde_json::json!(t));
            }
            if let Some(tier) = result.extraction_tier {
                f.insert("extraction_tier".to_string(), serde_json::json!(tier));
            }
            f.insert("spans".to_string(), spans);
            f.insert("total_duration_ms".to_string(), serde_json::json!(total_ms));
            f
        },
    });

    messages
}

/// Sync state back to shared session and broadcast messages to other players.
#[tracing::instrument(name = "turn.sync_session", skip_all)]
async fn sync_back_to_shared_session(
    ctx: &mut DispatchContext<'_>,
    messages: &[GameMessage],
    _clean_narration: &str,
    char_class: &str,
    effective_action: &str,
) {
    let holder = ctx.shared_session_holder.lock().await;
    if let Some(ref ss_arc) = *holder {
        let mut ss = ss_arc.lock().await;
        ss.sync_from_locals(
            ctx.current_location,
            ctx.npc_registry,
            ctx.narration_history,
            ctx.discovered_regions,
            ctx.trope_states,
            ctx.player_id,
        );
        // Sync acting player's character data to PlayerState for other players' PARTY_STATUS
        if let Some(ps) = ss.players.get_mut(ctx.player_id) {
            ps.character_hp = *ctx.hp;
            ps.character_max_hp = *ctx.max_hp;
            ps.character_level = *ctx.level;
            ps.character_xp = *ctx.xp;
            ps.character_class = char_class.to_string();
            ps.inventory = ctx.inventory.clone();
            ps.combat_state = ctx.combat_state.clone();
            ps.chase_state = ctx.chase_state.clone();
            if ps.character_name.is_none() {
                ps.character_name = Some(ctx.char_name.to_string());
            }
        }
        // Route messages to session members.
        // The acting player already receives via their direct tx channel (mpsc).
        // Other players get narration (without state_delta) via the session broadcast channel.
        // Fall back to all session members when cartography regions aren't available.
        let co_located = ss.co_located_players(ctx.player_id);
        let other_players: Vec<String> = if co_located.is_empty() {
            // No region data — fall back to all other session members
            ss.players
                .keys()
                .filter(|pid| pid.as_str() != ctx.player_id)
                .cloned()
                .collect()
        } else {
            co_located
        };
        for msg in messages {
            match msg {
                GameMessage::Narration { payload, .. } => {
                    // Send the acting player's action to observers FIRST.
                    // This creates a turn boundary in NarrativeView (PLAYER_ACTION triggers flushChunks).
                    let observer_action = GameMessage::PlayerAction {
                        payload: sidequest_protocol::PlayerActionPayload {
                            action: format!("{} — {}", ctx.char_name, effective_action),
                            aside: ctx.aside,
                        },
                        player_id: ctx.player_id.to_string(),
                    };
                    tracing::info!(
                        ctx.char_name = %ctx.char_name,
                        observer_count = other_players.len(),
                        "multiplayer.observer_action — broadcasting PLAYER_ACTION to observers"
                    );
                    for target_id in &other_players {
                        ss.send_to_player(observer_action.clone(), target_id.clone());
                    }
                    // Send narration (state_delta stripped) to other players.
                    // Apply perception rewriting if active filters exist (Story 15-4).
                    for target_id in &other_players {
                        let text = if let Some(filter) = ss.perception_filters.get(target_id) {
                            if filter.has_effects() {
                                // Use Claude-backed perception rewriter for actual narration variant
                                let client = sidequest_agents::client::ClaudeClient::new();
                                let strategy =
                                    sidequest_agents::agents::resonator::ClaudeRewriteStrategy::new(
                                        client,
                                    );
                                let rewriter = sidequest_game::perception::PerceptionRewriter::new(
                                    Box::new(strategy),
                                );
                                match rewriter.rewrite(&payload.text, filter, ctx.genre_slug) {
                                    Ok(rewritten) => {
                                        tracing::info!(
                                            target_player = %target_id,
                                            effects = %sidequest_game::perception::PerceptionRewriter::describe_effects(filter.effects()),
                                            "perception.rewrite — narration rewritten for player"
                                        );
                                        rewritten
                                    }
                                    Err(e) => {
                                        // Graceful degradation per ADR-006: fall back to annotated narration
                                        tracing::warn!(
                                            target_player = %target_id,
                                            error = %e,
                                            "perception.rewrite_failed — falling back to base narration"
                                        );
                                        let effects_desc = sidequest_game::perception::PerceptionRewriter::describe_effects(filter.effects());
                                        format!(
                                            "[Your perception is altered: {}]\n\n{}",
                                            effects_desc, payload.text
                                        )
                                    }
                                }
                            } else {
                                payload.text.clone()
                            }
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
                                player_name: ctx.player_name_for_save.to_string(),
                                status: "resolved".into(),
                                state_delta: None,
                            },
                            player_id: ctx.player_id.to_string(),
                        };
                        let _ = ctx.state.broadcast(resolved_msg);
                        tracing::info!(player_name = %ctx.player_name_for_save, "turn_status.resolved broadcast to all clients");
                    }
                }
                GameMessage::ChapterMarker { ref payload, .. } => {
                    // Send to other players only — acting player already received via direct channel
                    for target_pid in ss
                        .players
                        .keys()
                        .filter(|pid| pid.as_str() != ctx.player_id)
                    {
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
                            current_location: ps.display_location.clone(),
                        })
                        .collect();
                    let player_ids: Vec<String> = ss.players.keys().cloned().collect();
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
                _ => {}
            }
        }
    }
}

/// Audio/music — use narrator's scene_mood, or fall back to MusicDirector classification.
#[tracing::instrument(name = "turn.audio", skip_all)]
async fn process_audio(
    ctx: &mut DispatchContext<'_>,
    clean_narration: &str,
    messages: &mut Vec<GameMessage>,
    result: &sidequest_agents::orchestrator::ActionResult,
) {
    if let Some(ref mut director) = ctx.music_director {
        tracing::info!("music_director_present — evaluating mood");
        let mood_ctx = sidequest_game::MoodContext {
            in_combat: ctx.combat_state.in_combat(),
            in_chase: ctx.chase_state.is_some(),
            party_health_pct: if *ctx.max_hp > 0 {
                *ctx.hp as f32 / *ctx.max_hp as f32
            } else {
                1.0
            },
            // Quest completion and NPC death now come from structured quest_updates,
            // not keyword scanning. Check if any quest was marked "completed:".
            quest_completed: result.quest_updates.values().any(|v| v.starts_with("completed")),
            npc_died: ctx.npc_registry.iter().any(|n| n.max_hp > 0 && n.hp <= 0),
        };

        // Get telemetry snapshot BEFORE evaluate() changes state
        let pre_telemetry = director.telemetry_snapshot();
        let mood_reasoning = director.classify_mood_with_reasoning(clean_narration, &mood_ctx);

        // Use narrator's scene_mood — it's required every turn.
        let mood_key = match result.scene_mood.as_deref() {
            Some(mood) => {
                tracing::info!(mood = %mood, "music_mood_from_narrator");
                mood
            }
            None => {
                tracing::error!("narrator did not provide scene_mood — defaulting to exploration");
                "exploration"
            }
        };
        tracing::info!(
            mood = mood_key,
            in_combat = mood_ctx.in_combat,
            "music_mood_classified"
        );

        // Get turn_number for watcher event (approximate from turn_manager)
        let turn_approx = ctx.turn_manager.interaction();

        if let Some(cue) = director.evaluate(clean_narration, &mood_ctx) {
            tracing::info!(
                mood = mood_key,
                track = ?cue.track_id,
                ctx.action = %cue.action,
                volume = cue.volume,
                "music_cue_produced"
            );

            // Emit rich music telemetry to watcher
            ctx.state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "music_director".to_string(),
                event_type: WatcherEventType::AgentSpanClose,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert("turn_number".to_string(), serde_json::json!(turn_approx));
                    f.insert("mood_classified".to_string(), serde_json::json!(mood_reasoning.classification.primary.as_key()));
                    f.insert("mood_reason".to_string(), serde_json::json!(mood_reasoning.reason));
                    f.insert("narrator_scene_mood".to_string(), serde_json::json!(mood_key));
                    f.insert("intensity".to_string(), serde_json::json!(mood_reasoning.classification.intensity));
                    f.insert("confidence".to_string(), serde_json::json!(mood_reasoning.classification.confidence));
                    if !mood_reasoning.keyword_matches.is_empty() {
                        f.insert("keyword_matches".to_string(), serde_json::json!(
                            mood_reasoning.keyword_matches.iter()
                                .map(|(mood, kw)| format!("{}:{}", mood, kw))
                                .collect::<Vec<_>>()
                        ));
                    }
                    f.insert("track_selected".to_string(), serde_json::json!(cue.track_id));
                    f.insert("previous_mood".to_string(), serde_json::json!(pre_telemetry.current_mood));
                    f.insert("previous_track".to_string(), serde_json::json!(pre_telemetry.current_track));
                    f.insert("action".to_string(), serde_json::json!(cue.action.to_string()));
                    f.insert("volume".to_string(), serde_json::json!(cue.volume));
                    f.insert("rotation_history".to_string(), serde_json::json!(pre_telemetry.rotation_history));
                    f.insert("tracks_per_mood".to_string(), serde_json::json!(pre_telemetry.tracks_per_mood));
                    f
                },
            });

            let mixer_cues = {
                let mut mixer_guard = ctx.audio_mixer.lock().await;
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
                    ctx.player_id,
                    ctx.genre_slug,
                    Some(mood_key),
                ));
            }
        } else {
            // Mood didn't change — still emit telemetry so dashboard shows suppression
            ctx.state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "music_director".to_string(),
                event_type: WatcherEventType::AgentSpanClose,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert("turn_number".to_string(), serde_json::json!(turn_approx));
                    f.insert("mood_classified".to_string(), serde_json::json!(mood_reasoning.classification.primary.as_key()));
                    f.insert("mood_reason".to_string(), serde_json::json!(mood_reasoning.reason));
                    f.insert("narrator_scene_mood".to_string(), serde_json::json!(mood_key));
                    f.insert("suppressed".to_string(), serde_json::json!(true));
                    f.insert("suppression_reason".to_string(), serde_json::json!("same_mood_low_intensity"));
                    f.insert("current_mood".to_string(), serde_json::json!(pre_telemetry.current_mood));
                    f.insert("current_track".to_string(), serde_json::json!(pre_telemetry.current_track));
                    f
                },
            });
            tracing::warn!(
                mood = mood_key,
                "music_evaluate_returned_none — no cue produced"
            );
        }
    } else {
        tracing::warn!("music_director_missing — audio cues skipped");
    }
}

/// Render pipeline — use narrator's visual_scene for image prompts.
#[tracing::instrument(name = "turn.render", skip_all)]
async fn process_render(
    ctx: &mut DispatchContext<'_>,
    clean_narration: &str,
    narration_text: &str,
    result: &sidequest_agents::orchestrator::ActionResult,
) {
    // Use narrator's visual_scene — the narrator already imagined the scene.
    let scene = match result.visual_scene {
        Some(ref vs) => vs,
        None => {
            tracing::error!("narrator did not provide visual_scene — skipping render");
            return;
        }
    };

    // Map narrator tier string to SubjectTier
    let tier = match scene.tier.as_str() {
        "portrait" => sidequest_game::SubjectTier::Portrait,
        "landscape" => sidequest_game::SubjectTier::Landscape,
        "scene_illustration" => sidequest_game::SubjectTier::Scene,
        _ => sidequest_game::SubjectTier::Scene,
    };

    // Build RenderSubject from narrator's visual description
    let subject = match sidequest_game::RenderSubject::new(
        vec![], // entities not needed — the subject text is already visual
        sidequest_game::SceneType::Exploration,
        tier,
        scene.subject.clone(),
        0.6, // default weight — narrator provided, always worth rendering
    ) {
        Some(s) => s,
        None => {
            tracing::error!(subject = %scene.subject, "invalid visual_scene from narrator");
            return;
        }
    };

    tracing::info!(
        prompt = %subject.prompt_fragment(),
        tier = ?subject.tier(),
        "visual_scene from narrator"
    );

    let filter_ctx = sidequest_game::FilterContext {
        in_combat: ctx.combat_state.in_combat(),
        scene_transition: extract_location_header(narration_text).is_some(),
        player_requested: false,
    };
    let decision = ctx
        .state
        .inner
        .beat_filter
        .lock()
        .await
        .evaluate(&subject, &filter_ctx);
    tracing::info!(decision = ?decision, "BeatFilter decision");
    if matches!(decision, sidequest_game::FilterDecision::Render { .. }) {
        if let Some(ref queue) = ctx.state.inner.render_queue {
            let (art_style, model, neg_prompt) = match ctx.visual_style {
                Some(ref vs) => {
                    let location = extract_location_header(narration_text)
                        .unwrap_or_default()
                        .to_lowercase();
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
                    (
                        style,
                        vs.preferred_model.clone(),
                        vs.negative_prompt.clone(),
                    )
                }
                None => (
                    "oil_painting".to_string(),
                    "flux-schnell".to_string(),
                    String::new(),
                ),
            };
            // Send visual_scene subject as prompt — no narration, daemon skips SubjectExtractor
            match queue
                .enqueue(subject, &art_style, &model, &neg_prompt, "")
                .await
            {
                Ok(r) => tracing::info!(result = ?r, "Render job enqueued"),
                Err(e) => tracing::warn!(error = %e, "Render enqueue failed"),
            }
        }
    }
}

/// Scan narration for trope triggers, tick the trope engine.
fn process_tropes(
    ctx: &mut DispatchContext<'_>,
    clean_narration: &str,
    _messages: &mut Vec<GameMessage>,
) {
    let _span =
        tracing::info_span!("turn.tropes", active_count = ctx.trope_states.len(),).entered();

    let narration_lower = clean_narration.to_lowercase();
    tracing::debug!(
        narration_len = narration_lower.len(),
        active_tropes = ctx.trope_states.len(),
        total_defs = ctx.trope_defs.len(),
        "Trope keyword scan starting"
    );
    for def in ctx.trope_defs.iter() {
        let id = match &def.id {
            Some(id) => id,
            None => continue,
        };
        // Skip already active tropes
        if ctx
            .trope_states
            .iter()
            .any(|ts| ts.trope_definition_id() == id)
        {
            continue;
        }
        // Check if any trigger keyword appears in the narration
        let triggered = def
            .triggers
            .iter()
            .any(|t| narration_lower.contains(&t.to_lowercase()));
        if triggered {
            sidequest_game::trope::TropeEngine::activate(ctx.trope_states, id);
            tracing::info!(trope_id = %id, "Trope activated by narration keyword");
            ctx.state.send_watcher_event(WatcherEvent {
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
    for ts in ctx.trope_states.iter() {
        tracing::info!(
            trope_id = %ts.trope_definition_id(),
            status = ?ts.status(),
            progression = ts.progression(),
            fired_beats = ts.fired_beats().len(),
            "Trope pre-tick state"
        );
    }
    let fired = sidequest_game::trope::TropeEngine::tick(ctx.trope_states, ctx.trope_defs);
    sidequest_game::trope::TropeEngine::apply_keyword_modifiers(
        ctx.trope_states,
        ctx.trope_defs,
        clean_narration,
    );
    tracing::info!(
        active_tropes = ctx.trope_states.len(),
        fired_beats = fired.len(),
        "Trope tick complete"
    );
    // Log post-tick state
    for ts in ctx.trope_states.iter() {
        tracing::debug!(
            trope_id = %ts.trope_definition_id(),
            status = ?ts.status(),
            progression = ts.progression(),
            "Trope post-tick state"
        );
    }
    for beat in &fired {
        tracing::info!(trope = %beat.trope_name, "Trope beat fired");
        ctx.state.send_watcher_event(WatcherEvent {
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
}

/// Combat detection, combat tick, combat overlay, chase detection.
#[tracing::instrument(name = "turn.combat_and_chase", skip_all)]
async fn process_combat_and_chase(
    ctx: &mut DispatchContext<'_>,
    clean_narration: &str,
    result: &sidequest_agents::orchestrator::ActionResult,
    messages: &mut Vec<GameMessage>,
) {
    // Combat engagement is driven by CombatPatch from creature_smith (in apply_state_mutations),
    // NOT by intent classification. Intent classification routes to the right agent, but the
    // agent decides whether combat actually starts via the in_combat field in its patch.
    //
    // Keyword fallback: narration explicitly describes combat starting (emergency only).
    // This catches cases where the narrator (not creature_smith) describes combat beginning.
    if !ctx.combat_state.in_combat() {
        let narr_lower = clean_narration.to_lowercase();
        let combat_start_keywords = ["roll for initiative", "combat begins", "enters combat"];
        if combat_start_keywords
            .iter()
            .any(|kw| narr_lower.contains(kw))
        {
            // Extract combatant names from hp_changes in combat patch (if present),
            // otherwise use only the player name — don't dump all known NPCs.
            let mut combatants: Vec<String> = vec![ctx.char_name.to_string()];
            if let Some(ref combat_patch) = result.combat_patch {
                if let Some(ref hp_changes) = combat_patch.hp_changes {
                    for target in hp_changes.keys() {
                        if !combatants.iter().any(|c| c.eq_ignore_ascii_case(target)) {
                            combatants.push(target.clone());
                        }
                    }
                }
            }
            ctx.combat_state.engage(combatants);
            tracing::info!(
                source = "keyword",
                turn_order = ?ctx.combat_state.turn_order(),
                current_turn = ?ctx.combat_state.current_turn(),
                "combat.engaged"
            );
            // Transition turn mode: FreePlay → Structured
            {
                let holder = ctx.shared_session_holder.lock().await;
                if let Some(ref ss_arc) = *holder {
                    let mut ss = ss_arc.lock().await;
                    let old_mode = std::mem::take(&mut ss.turn_mode);
                    ss.turn_mode = old_mode
                        .apply(sidequest_game::turn_mode::TurnModeTransition::CombatStarted);
                    tracing::info!(new_mode = ?ss.turn_mode, "Turn mode transitioned on combat start");
                    if ss.turn_mode.should_use_barrier() && ss.turn_barrier.is_none() {
                        let mp_session =
                            sidequest_game::multiplayer::MultiplayerSession::with_player_ids(
                                ss.players.keys().cloned(),
                            );
                        let adaptive = sidequest_game::barrier::AdaptiveTimeout::default();
                        ss.turn_barrier =
                            Some(sidequest_game::barrier::TurnBarrier::with_adaptive(
                                mp_session, adaptive,
                            ));
                    }
                }
            }
        }
    }

    // Combat end detection — keyword fallback for narration-driven combat end
    if ctx.combat_state.in_combat() {
        let narr_lower = clean_narration.to_lowercase();
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
        if combat_end_keywords.iter().any(|kw| narr_lower.contains(kw)) {
            ctx.combat_state.disengage();
            tracing::info!("combat.disengaged — detected end keyword in narration");
            // Transition turn mode: Structured → FreePlay
            {
                let holder = ctx.shared_session_holder.lock().await;
                if let Some(ref ss_arc) = *holder {
                    let mut ss = ss_arc.lock().await;
                    let old_mode = std::mem::take(&mut ss.turn_mode);
                    ss.turn_mode =
                        old_mode.apply(sidequest_game::turn_mode::TurnModeTransition::CombatEnded);
                    tracing::info!(new_mode = ?ss.turn_mode, "Turn mode transitioned on combat end");
                }
            }
        }
    }

    // Combat tick — tick status effects (round advancement handled by advance_turn in apply_state_mutations)
    let was_in_combat = ctx.combat_state.in_combat();
    tracing::debug!(
        in_combat = was_in_combat,
        round = ctx.combat_state.round(),
        drama_weight = ctx.combat_state.drama_weight(),
        "combat.pre_tick"
    );
    if ctx.combat_state.in_combat() {
        ctx.combat_state.tick_effects();
        ctx.state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "combat".to_string(),
            event_type: WatcherEventType::AgentSpanOpen,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert(
                    "round".to_string(),
                    serde_json::json!(ctx.combat_state.round()),
                );
                f.insert(
                    "drama_weight".to_string(),
                    serde_json::json!(ctx.combat_state.drama_weight()),
                );
                f.insert(
                    "turn_order".to_string(),
                    serde_json::json!(ctx.combat_state.turn_order()),
                );
                f.insert(
                    "current_turn".to_string(),
                    serde_json::json!(ctx.combat_state.current_turn()),
                );
                f
            },
        });
    }

    // Combat overlay — send populated CombatEvent with enemies, turn order, current turn
    if was_in_combat || ctx.combat_state.in_combat() {
        let enemies: Vec<sidequest_protocol::CombatEnemy> = ctx
            .npc_registry
            .iter()
            .filter(|_| ctx.combat_state.in_combat()) // only show enemies during active combat
            .map(|entry| sidequest_protocol::CombatEnemy {
                name: entry.name.clone(),
                hp: entry.hp,
                max_hp: entry.max_hp,
                ac: None,
            })
            .collect();
        messages.push(GameMessage::CombatEvent {
            payload: CombatEventPayload {
                in_combat: ctx.combat_state.in_combat(),
                enemies,
                turn_order: ctx.combat_state.turn_order().to_vec(),
                current_turn: ctx.combat_state.current_turn().unwrap_or("").to_string(),
            },
            player_id: ctx.player_id.to_string(),
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

        if let Some(ref mut cs) = ctx.chase_state {
            // Update active chase
            if chase_end_keywords.iter().any(|kw| narr_lower.contains(kw)) {
                tracing::info!(rounds = cs.round(), "Chase resolved");
                *ctx.chase_state = None;
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
                tracing::info!(
                    round = cs.round(),
                    separation = cs.separation(),
                    gain,
                    "chase.tick — round advanced"
                );
            }
        } else if chase_start_keywords
            .iter()
            .any(|kw| narr_lower.contains(kw))
        {
            let cs = sidequest_game::ChaseState::new(sidequest_game::ChaseType::Footrace, 0.5);
            tracing::info!("Chase started — detected chase keyword in narration");
            *ctx.chase_state = Some(cs);
        }
    }
}

/// Apply post-narration state mutations: combat HP, quests, XP, affinity, items.
fn apply_state_mutations(
    ctx: &mut DispatchContext<'_>,
    result: &sidequest_agents::orchestrator::ActionResult,
    clean_narration: &str,
    effective_action: &str,
) -> Vec<sidequest_game::AffinityTierUpEvent> {
    let _span = tracing::info_span!("turn.state_mutations").entered();
    let mut all_tier_events = Vec::new();

    // Combat state — apply typed CombatPatch from creature_smith
    if let Some(ref combat_patch) = result.combat_patch {
        // Combat start → engage() with player + NPCs from the patch (not all known NPCs)
        if let Some(in_combat) = combat_patch.in_combat {
            if in_combat && !ctx.combat_state.in_combat() {
                // Build combatant list from the patch, not from npc_registry.
                // Prefer turn_order if provided; otherwise use hp_changes targets.
                let combatants = if combat_patch
                    .turn_order
                    .as_ref()
                    .map_or(false, |o| !o.is_empty())
                {
                    combat_patch.turn_order.clone().unwrap()
                } else {
                    let mut names: Vec<String> = vec![ctx.char_name.to_string()];
                    if let Some(ref hp_changes) = combat_patch.hp_changes {
                        for target in hp_changes.keys() {
                            if !names.iter().any(|n| n.eq_ignore_ascii_case(target)) {
                                names.push(target.clone());
                            }
                        }
                    }
                    names
                };
                ctx.combat_state.engage(combatants);
                tracing::info!(
                    turn_order = ?ctx.combat_state.turn_order(),
                    current_turn = ?ctx.combat_state.current_turn(),
                    "combat.engaged"
                );
            } else if !in_combat && ctx.combat_state.in_combat() {
                ctx.combat_state.disengage();
                tracing::info!("combat.disengaged");
            }
        }

        // Apply HP deltas
        if let Some(ref hp_changes) = combat_patch.hp_changes {
            let char_name_lower = ctx.player_name_for_save.to_lowercase();
            for (target, delta) in hp_changes {
                let target_lower = target.to_lowercase();
                if target_lower == char_name_lower
                    || ctx
                        .character_json
                        .as_ref()
                        .and_then(|cj| cj.get("name"))
                        .and_then(|n| n.as_str())
                        .map(|n| n.to_lowercase() == target_lower)
                        .unwrap_or(false)
                {
                    *ctx.hp = sidequest_game::clamp_hp(*ctx.hp, *delta, *ctx.max_hp);
                    tracing::info!(target = %target, delta = delta, new_hp = *ctx.hp, "combat.patch.hp_applied");
                } else if let Some(npc) = ctx.npc_registry.iter_mut().find(|n| n.name.to_lowercase() == target_lower) {
                    // Initialize NPC max_hp on first damage if not yet set
                    if npc.max_hp == 0 {
                        // Estimate: if the LLM is dealing damage, assume NPC has some HP.
                        // Set max_hp to a reasonable default so clamp_hp works.
                        npc.max_hp = 20;
                        npc.hp = npc.max_hp;
                    }
                    npc.hp = sidequest_game::clamp_hp(npc.hp, *delta, npc.max_hp);
                    tracing::info!(target = %target, delta = delta, new_hp = npc.hp, max_hp = npc.max_hp, "combat.patch.npc_hp_applied");
                }
            }
        }

        // Apply turn_order/current_turn updates (mid-combat changes)
        if ctx.combat_state.in_combat() {
            if let Some(ref order) = combat_patch.turn_order {
                if !order.is_empty() {
                    ctx.combat_state.set_turn_order(order.clone());
                }
            }
            if let Some(ref turn) = combat_patch.current_turn {
                ctx.combat_state.set_current_turn(turn.clone());
            }
        }

        if let Some(dw) = combat_patch.drama_weight {
            ctx.combat_state.set_drama_weight(dw);
        }

        // Advance turn (handles round wrap internally)
        if combat_patch.advance_round && ctx.combat_state.in_combat() {
            ctx.combat_state.advance_turn();
        }
    }

    // Quest log updates — merge narrator-extracted quest changes
    if !result.quest_updates.is_empty() {
        for (quest_name, status) in &result.quest_updates {
            ctx.quest_log.insert(quest_name.clone(), status.clone());
            tracing::info!(quest = %quest_name, status = %status, "quest.updated");
        }
    }

    // Bug 3: XP award based on action type
    {
        let xp_award = if ctx.combat_state.in_combat() {
            25 // combat actions give more XP
        } else {
            10 // exploration/dialogue gives base XP
        };
        *ctx.xp += xp_award;
        tracing::info!(
            xp_award = xp_award,
            total_xp = *ctx.xp,
            ctx.level = *ctx.level,
            "XP awarded"
        );

        // Check for level up
        let threshold = sidequest_game::xp_for_level(*ctx.level + 1);
        if *ctx.xp >= threshold {
            *ctx.level += 1;
            let new_max_hp = sidequest_game::level_to_hp(10, *ctx.level);
            let hp_gain = new_max_hp - *ctx.max_hp;
            *ctx.max_hp = new_max_hp;
            *ctx.hp = sidequest_game::clamp_hp(*ctx.hp + hp_gain, 0, *ctx.max_hp);
            tracing::info!(
                new_level = *ctx.level,
                new_max_hp = *ctx.max_hp,
                hp_gain = hp_gain,
                "Level up!"
            );
        }
    }

    // Affinity progression (Story F8) — check thresholds after XP/level-up.
    // Loads genre pack affinities via state to avoid adding another parameter.
    if let Some(ref cj) = ctx.character_json {
        if let Ok(mut ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
            // Sync mutable fields
            ch.core.hp = *ctx.hp;
            ch.core.max_hp = *ctx.max_hp;
            ch.core.level = *ctx.level;
            ch.core.inventory = ctx.inventory.clone();

            // Increment affinity progress for any matching action triggers.
            let genre_code = sidequest_genre::GenreCode::new(ctx.genre_slug);
            if let Ok(code) = genre_code {
                let loader = GenreLoader::new(vec![ctx.state.genre_packs_path().to_path_buf()]);
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
                        ctx.char_name,
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
                    all_tier_events.extend(tier_events);
                }
            } // if let Ok(code)

            // Write updated character back to character_json
            if let Ok(updated_json) = serde_json::to_value(&ch) {
                *ctx.character_json = Some(updated_json);
            }
        }
    }

    // Item acquisition — driven by structured extraction from the LLM response.
    // The narrator emits items_gained in its JSON block when the player
    // actually acquires something.
    const VALID_ITEM_CATEGORIES: &[&str] = &[
        "weapon",
        "armor",
        "tool",
        "consumable",
        "quest",
        "treasure",
        "misc",
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
        if ctx.inventory.find(&item_id).is_some() {
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
            let _ = ctx.inventory.add(item, 50);
            tracing::info!(item_name = %item_def.name, "Item added to inventory from LLM extraction");
        }
    }

    // Resource delta application (story 16-1)
    if !result.resource_deltas.is_empty() {
        for (name, delta) in &result.resource_deltas {
            if let Some(current) = ctx.resource_state.get_mut(name) {
                *current += delta;
                // Clamp to bounds if declaration exists
                if let Some(decl) = ctx.resource_declarations.iter().find(|d| d.name == *name) {
                    *current = current.clamp(decl.min, decl.max);
                }
                tracing::info!(resource = %name, delta = %delta, new_value = %current, "resource.delta_applied");
            } else {
                tracing::debug!(resource = %name, "resource.delta_ignored — resource not in state");
            }
        }
    }

    all_tier_events
}

/// Build the full state_summary string for the narrator prompt.
/// Includes trope seeding, party roster, location constraints, inventory, quests,
/// chase state, abilities, world context, regions, tone, history, NPCs, lore, and
/// continuity corrections.
#[tracing::instrument(name = "turn.build_prompt_context", skip_all)]
async fn build_prompt_context(ctx: &mut DispatchContext<'_>) -> String {
    // Seed starter tropes if none are active yet (first turn)
    if ctx.trope_states.is_empty() && !ctx.trope_defs.is_empty() {
        // Prefer tropes with passive_progression so tick() can advance them.
        // Fall back to any trope if none have passive_progression.
        let mut seedable: Vec<&sidequest_genre::TropeDefinition> = ctx
            .trope_defs
            .iter()
            .filter(|d| d.passive_progression.is_some() && d.id.is_some())
            .collect();
        if seedable.is_empty() {
            seedable = ctx.trope_defs.iter().filter(|d| d.id.is_some()).collect();
        }
        let seed_count = seedable.len().min(3);
        tracing::info!(
            total_defs = ctx.trope_defs.len(),
            with_progression = ctx
                .trope_defs
                .iter()
                .filter(|d| d.passive_progression.is_some())
                .count(),
            seedable = seedable.len(),
            seed_count = seed_count,
            "Trope seeding — selecting starter tropes"
        );
        for def in &seedable[..seed_count] {
            if let Some(id) = &def.id {
                sidequest_game::trope::TropeEngine::activate(ctx.trope_states, id);
                tracing::info!(
                    trope_id = %id,
                    name = %def.name,
                    has_progression = def.passive_progression.is_some(),
                    "Seeded starter trope"
                );
                ctx.state.send_watcher_event(WatcherEvent {
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
    let trope_context = if ctx.trope_states.is_empty() {
        String::new()
    } else {
        let mut lines = vec!["Active narrative arcs:".to_string()];
        for ts in ctx.trope_states.iter() {
            if let Some(def) = ctx
                .trope_defs
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
        ctx.char_name, *ctx.hp, *ctx.max_hp, *ctx.level, *ctx.xp, ctx.genre_slug,
    );

    // Inject party roster so the narrator knows which characters are player-controlled
    // and never puppets them (gives them dialogue, actions, or internal state).
    {
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            let other_pcs: Vec<String> = ss
                .players
                .iter()
                .filter(|(pid, _)| pid.as_str() != ctx.player_id)
                .filter_map(|(_, ps)| ps.character_name.clone())
                .collect();
            let co_located_names: Vec<String> = ss
                .co_located_players(ctx.player_id)
                .iter()
                .filter_map(|pid| {
                    ss.players
                        .get(pid.as_str())
                        .and_then(|ps| ps.character_name.clone())
                })
                .collect();

            if !other_pcs.is_empty() {
                state_summary.push_str("\n\nPLAYER-CONTROLLED CHARACTERS IN THE PARTY:\n");
                state_summary
                    .push_str("The following characters are controlled by OTHER human players:\n");
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
                     Wrong: 'You survey the gantry.'",
                );
            }
        }
    }

    // Location constraint — prevent narrator from teleporting between scenes
    if !ctx.current_location.is_empty() {
        // Dialogue context: if the player interacted with an NPC in the last 2 turns,
        // any location mention in the action is likely dialogue (describing a place to
        // the NPC), not a travel intent. Strengthen the stay-put constraint.
        let turn_approx = ctx.turn_manager.interaction() as u32;
        let recent_npc_interaction = ctx
            .npc_registry
            .iter()
            .any(|e| turn_approx.saturating_sub(e.last_seen_turn) <= 2);
        let extra_dialogue_guard = if recent_npc_interaction {
            " IMPORTANT: The player is currently in dialogue with an NPC. If the player's \
             ctx.action mentions a location or place name, they are TALKING ABOUT that place, \
             NOT traveling there. Keep the scene at the current location. Only move if the \
             player explicitly ends the conversation and states they are leaving."
        } else {
            ""
        };
        state_summary.push_str(&format!(
            "\n\nLOCATION CONSTRAINT — THIS IS A HARD RULE:\nThe player is at: {}\nYou MUST continue the scene at this location. Do NOT introduce a new setting, move to a different area, or describe the player arriving somewhere else UNLESS the player explicitly says they want to travel or leave. If the player's action implies staying here, describe what happens HERE. Only change location when the player takes a deliberate travel action (e.g., 'I go to...', 'I leave...', 'I head north').{}",
            ctx.current_location, extra_dialogue_guard
        ));
    }

    // Inventory constraint — the narrator must respect the character sheet
    let equipped_count = ctx.inventory.items.iter().filter(|i| i.equipped).count();
    tracing::debug!(
        items = ctx.inventory.items.len(),
        equipped = equipped_count,
        gold = ctx.inventory.gold,
        "narrator_prompt.inventory_constraint — injecting character sheet"
    );
    state_summary.push_str("\n\nCHARACTER SHEET — INVENTORY (canonical, overrides narration):");
    if !ctx.inventory.items.is_empty() {
        state_summary.push_str("\nThe player currently possesses EXACTLY these items:");
        for item in &ctx.inventory.items {
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
        state_summary.push_str(&format!("\nGold: {}", ctx.inventory.gold));
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
    if !ctx.quest_log.is_empty() {
        state_summary.push_str("\n\nACTIVE QUESTS:\n");
        for (quest_name, status) in ctx.quest_log.iter() {
            state_summary.push_str(&format!("- {}: {}\n", quest_name, status));
        }
        state_summary.push_str("Reference active quests when narratively relevant. Update quest status in quest_updates when objectives change.\n");
    }

    // Resource state injection (story 16-1)
    if !ctx.resource_declarations.is_empty() {
        state_summary.push_str("\n\nGENRE RESOURCES — Current State:\n");
        for decl in ctx.resource_declarations {
            let current = ctx
                .resource_state
                .get(&decl.name)
                .copied()
                .unwrap_or(decl.starting);
            let vol_label = if decl.voluntary {
                "voluntary"
            } else {
                "involuntary"
            };
            let mut line = format!("{}: {}/{} ({})", decl.label, current, decl.max, vol_label);
            if decl.decay_per_turn.abs() > f64::EPSILON {
                line.push_str(&format!(", decay {}/turn", decl.decay_per_turn.abs()));
            }
            state_summary.push_str(&format!("- {}\n", line));
        }
        state_summary.push_str("When narrative events affect these resources, include resource_deltas in your JSON block.\n");
    }

    // Bug 6: Include chase state if active
    if let Some(ref cs) = ctx.chase_state {
        state_summary.push_str(&format!(
            "\nACTIVE CHASE: {:?} (round {}, separation {})",
            cs.chase_type(),
            cs.round(),
            cs.separation()
        ));
    }

    // Include character abilities and mutations so the narrator knows what
    // the character can and cannot do (prevents hallucinated abilities).
    if let Some(ref cj) = ctx.character_json {
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
                state_summary.push_str(&format!(
                    "\nPronouns: {} — ALWAYS use these pronouns for this character.",
                    pronouns
                ));
                tracing::debug!(pronouns = %pronouns, "narrator_prompt.pronouns — injected into state_summary");
            }
        }
    }

    if !ctx.world_context.is_empty() {
        state_summary.push('\n');
        state_summary.push_str(ctx.world_context);
    }

    // Inject known locations so the narrator uses canonical place names
    if !ctx.discovered_regions.is_empty() {
        state_summary.push_str("\n\nKNOWN LOCATIONS IN THIS WORLD:\n");
        state_summary.push_str("Use ONLY these location names when referring to places the party has visited or heard about. Do NOT invent new settlement names.\n");
        for region in ctx.discovered_regions.iter() {
            state_summary.push_str(&format!("- {}\n", region));
        }
    }
    // Also inject cartography region names from the shared session (if available)
    {
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            if !ss.region_names.is_empty() {
                if ctx.discovered_regions.is_empty() {
                    state_summary.push_str("\n\nWORLD LOCATIONS (from cartography):\n");
                    state_summary
                        .push_str("Use these canonical location names. Do NOT invent new ones.\n");
                } else {
                    state_summary.push_str("Additional world locations (not yet visited):\n");
                }
                for (region_id, _display_name) in &ss.region_names {
                    if !ctx
                        .discovered_regions
                        .iter()
                        .any(|r| r.to_lowercase() == *region_id)
                    {
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
    if let Some(ref ac) = ctx.axes_config {
        let tone_text = sidequest_game::format_tone_context(ac, ctx.axis_values);
        if !tone_text.is_empty() {
            state_summary.push_str(&tone_text);
        }
    }

    // Bug 17: Include recent narration history so the narrator maintains continuity
    if !ctx.narration_history.is_empty() {
        state_summary.push_str("\n\nRECENT CONVERSATION HISTORY (multiple players, most recent last):\nEntries are tagged with [CharacterName]. Only narrate for the ACTING player — do not continue another player's scene:\n");
        // Include at most the last 10 turns to stay within context limits
        let start = ctx.narration_history.len().saturating_sub(10);
        for entry in &ctx.narration_history[start..] {
            state_summary.push_str(entry);
            state_summary.push('\n');
        }
    }

    // Inject NPC registry so the narrator maintains identity consistency
    let npc_context = build_npc_registry_context(ctx.npc_registry);
    if !npc_context.is_empty() {
        state_summary.push_str(&npc_context);
    }

    // Inject lore context from genre pack — budget-aware selection (story 11-4)
    {
        let context_hint = if !ctx.current_location.is_empty() {
            Some(ctx.current_location.as_str())
        } else {
            None
        };
        let lore_budget = 500; // ~500 tokens for lore context
        let selected =
            sidequest_game::select_lore_for_prompt(ctx.lore_store, lore_budget, context_hint);
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

    // Inject continuity corrections from the previous turn (if any)
    if !ctx.continuity_corrections.is_empty() {
        state_summary.push_str("\n\n");
        state_summary.push_str(ctx.continuity_corrections);
        tracing::info!(
            corrections_len = ctx.continuity_corrections.len(),
            "continuity.corrections_injected_to_prompt"
        );
        // Clear after injection — corrections are one-shot
        ctx.continuity_corrections.clear();
    }

    state_summary
}

/// Handle an aside — out-of-character commentary that does not affect the game world.
///
/// Calls the narrator with an aside-specific prompt injection, then returns narration
/// only. Skips ALL state mutation subsystems: no combat, no chase, no tropes, no
/// renders, no music, no NPC registry, no narration history, no turn barrier.
async fn handle_aside(ctx: &mut DispatchContext<'_>) -> Vec<GameMessage> {
    tracing::info!(player = %ctx.char_name, action = %ctx.action, "aside — out-of-character, skipping state mutations");

    // Build a minimal prompt context with the aside instruction injected.
    let mut state_summary = build_prompt_context(ctx).await;
    state_summary.push_str(concat!(
        "\n\nASIDE RULES (HARD CONSTRAINTS):",
        "\nThe player is speaking an aside — an out-of-character thought, whisper, or ",
        "meta-commentary. This is NOT an in-world action.",
        "\n1. Respond with a brief inner-monologue, fourth-wall-breaking quip, or flavor acknowledgment.",
        "\n2. Do NOT advance the story, trigger combat, move NPCs, or change ANY game state.",
        "\n3. Do NOT describe the character performing any actions or interacting with the world.",
        "\n4. Keep it short — 1-3 sentences maximum.",
    ));

    let context = TurnContext {
        state_summary: Some(state_summary),
        in_combat: ctx.combat_state.in_combat(),
        in_chase: ctx.chase_state.is_some(),
        narrator_verbosity: ctx.narrator_verbosity,
        narrator_vocabulary: ctx.narrator_vocabulary,
    };
    let result = ctx
        .state
        .game_service()
        .process_action(&format!("(aside) {}", ctx.action), &context);

    let narration_text = strip_location_header(&result.narration);

    // Return narration + end with no state_delta — the aside is ephemeral.
    vec![
        GameMessage::Narration {
            payload: NarrationPayload {
                text: narration_text.to_string(),
                state_delta: None,
                footnotes: vec![],
            },
            player_id: ctx.player_id.to_string(),
        },
        GameMessage::NarrationEnd {
            payload: NarrationEndPayload { state_delta: None },
            player_id: ctx.player_id.to_string(),
        },
    ]
}

/// Slash command interception — route /commands to mechanical handlers, not the LLM.
/// Returns `Some(messages)` for early return, `None` to continue normal dispatch.
fn handle_slash_command(ctx: &mut DispatchContext<'_>) -> Option<Vec<GameMessage>> {
    if !ctx.action.starts_with('/') {
        return None;
    }
    let _span = tracing::info_span!("turn.slash_command", command = %ctx.action).entered();

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
    if let Some(ref ac) = ctx.axes_config {
        router.register(Box::new(sidequest_game::ToneCommand::new(ac.clone())));
    }

    // Build a minimal GameSnapshot from the local session state.
    let snapshot = {
        let mut snap = GameSnapshot {
            genre_slug: ctx.genre_slug.to_string(),
            world_slug: ctx.world_slug.to_string(),
            location: ctx.current_location.clone(),
            combat: ctx.combat_state.clone(),
            chase: ctx.chase_state.clone(),
            axis_values: ctx.axis_values.clone(),
            active_tropes: ctx.trope_states.clone(),
            quest_log: ctx.quest_log.clone(),
            ..GameSnapshot::default()
        };
        // Reconstruct a minimal Character from loose variables.
        if let Some(ref cj) = ctx.character_json {
            if let Ok(mut ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
                // Sync mutable fields that may have diverged from the JSON snapshot.
                ch.core.hp = *ctx.hp;
                ch.core.max_hp = *ctx.max_hp;
                ch.core.level = *ctx.level;
                ch.core.inventory = ctx.inventory.clone();
                snap.characters.push(ch);
            }
        }
        snap
    };

    if let Some(cmd_result) = router.try_dispatch(ctx.action, &snapshot) {
        tracing::info!(command = %ctx.action, result_type = ?std::mem::discriminant(&cmd_result), "slash_command.dispatched");
        let text = match &cmd_result {
            sidequest_game::slash_router::CommandResult::Display(t) => t.clone(),
            sidequest_game::slash_router::CommandResult::Error(e) => e.clone(),
            sidequest_game::slash_router::CommandResult::StateMutation(patch) => {
                // Apply location/region changes from /gm commands.
                if let Some(ref loc) = patch.location {
                    *ctx.current_location = loc.clone();
                }
                if let Some(ref hp_changes) = patch.hp_changes {
                    for (_target, delta) in hp_changes {
                        *ctx.hp = (*ctx.hp + delta).max(0);
                    }
                }
                format!("GM command applied.")
            }
            sidequest_game::slash_router::CommandResult::ToneChange(new_values) => {
                *ctx.axis_values = new_values.clone();
                format!("Tone updated.")
            }
            _ => "Command executed.".to_string(),
        };

        // Watcher: slash command handled
        ctx.state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "game".to_string(),
            event_type: WatcherEventType::AgentSpanClose,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert(
                    "slash_command".to_string(),
                    serde_json::Value::String(ctx.action.to_string()),
                );
                f.insert("result_len".to_string(), serde_json::json!(text.len()));
                f
            },
        });

        return Some(vec![
            GameMessage::Narration {
                payload: NarrationPayload {
                    text,
                    state_delta: None,
                    footnotes: vec![],
                },
                player_id: ctx.player_id.to_string(),
            },
            GameMessage::NarrationEnd {
                payload: NarrationEndPayload { state_delta: None },
                player_id: ctx.player_id.to_string(),
            },
        ]);
    }

    None
}

//! Player action dispatch — the main game loop handler.
//!
//! Decomposed into submodules:
//! - `audio` — music mood classification and cue generation
//! - `combat` — combat/chase detection and state tracking
//! - `prompt` — narrator prompt context builder
//! - `render` — image render pipeline
//! - `session_sync` — shared session synchronization
//! - `slash` — slash command interception
//! - `state_mutations` — post-narration state mutations (HP, XP, items, etc.)
//! - `tropes` — trope engine (activation, tick, escalation)

mod audio;
mod combat;
pub(crate) mod connect;
mod prompt;
mod render;
mod session_sync;
mod slash;
mod state_mutations;
mod tropes;

use std::collections::HashMap;
use std::sync::Arc;

use tracing::Instrument;

use sidequest_agents::orchestrator::TurnContext;
use sidequest_genre::{GenreCode, GenreLoader};
use sidequest_protocol::{
    ActionRevealPayload, ChapterMarkerPayload, GameMessage, InventoryPayload, MapUpdatePayload,
    NarrationEndPayload, NarrationPayload, PartyMember, PartyStatusPayload, PlayerActionEntry,
    SessionEventPayload, ThinkingPayload, TurnStatusPayload,
};

use crate::extraction::{
    audio_cue_to_game_message, extract_location_header, strip_location_header,
    strip_markdown_for_tts,
};
use crate::{
    shared_session, AppState, DaemonSynthesizer, NpcRegistryEntry, Severity, WatcherEventBuilder,
    WatcherEventType,
};

/// Mutable per-player state passed through the dispatch pipeline.
pub(crate) struct DispatchContext<'a> {
    pub action: &'a str,
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
    pub lore_store: &'a mut sidequest_game::LoreStore,
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
    pub aside: bool,
    /// Opening scenario directive — injected into Early zone on turn 0 only, then consumed.
    pub opening_directive: Option<String>,
    /// SFX library from genre pack: ID → list of file paths.
    pub sfx_library: std::collections::HashMap<String, Vec<String>>,
    /// Room definitions for room_graph navigation mode (from cartography.rooms).
    /// Empty for region-based navigation.
    pub rooms: Vec<sidequest_genre::RoomDef>,
    pub narrator_verbosity: sidequest_protocol::NarratorVerbosity,
    pub narrator_vocabulary: sidequest_protocol::NarratorVocabulary,
    /// Genre pack affinity definitions — used by resolve_abilities() to map tiers to ability names.
    pub genre_affinities: Vec<sidequest_genre::Affinity>,
    pub pending_trope_context: &'a mut Option<String>,
    pub achievement_tracker: &'a mut sidequest_game::achievement::AchievementTracker,
    /// Canonical game state snapshot — patched in-place during the turn,
    /// saved directly by persist_game_state() without re-loading from SQLite.
    /// Story 15-8: eliminates the load-before-save round-trip on every turn.
    pub snapshot: &'a mut sidequest_game::state::GameSnapshot,
    /// Direct sender to the client WebSocket writer — used to emit narration
    /// immediately before state cleanup completes (approach A streaming).
    pub tx: &'a tokio::sync::mpsc::Sender<sidequest_protocol::GameMessage>,
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
                WatcherEventBuilder::new("multiplayer", WatcherEventType::AgentSpanOpen)
                    .field("event", "multiplayer_action")
                    .field("session_key", format!("{}:{}", ctx.genre_slug, ctx.world_slug))
                    .field("player_id", ctx.player_id)
                    .field("party_size", pc)
                    .send(ctx.state);
            }
        }
    }

    // Story 15-20: capture pre-turn snapshot for delta computation
    let before_snapshot = sidequest_game::delta::snapshot(ctx.snapshot);

    // Story 12-1: capture location before state updates for cinematic variation detection
    let location_before_turn = ctx.current_location.clone();

    // Timing capture for OTEL flame chart spans
    let turn_start = std::time::Instant::now();

    // Watcher: action received
    let turn_number = ctx.turn_manager.interaction();
    turn_span.record("turn_number", turn_number);
    WatcherEventBuilder::new("game", WatcherEventType::AgentSpanOpen)
        .field("action", ctx.action)
        .field("player", ctx.char_name)
        .field("turn_number", turn_number)
        .send(ctx.state);

    // TURN_STATUS "active" — tell all players whose turn it is BEFORE the LLM call.
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
            let _ = ctx.state.broadcast(thinking.clone());
        }
    }

    // Slash command interception — route /commands to mechanical handlers, not the LLM.
    if let Some(slash_messages) = slash::handle_slash_command(ctx) {
        return slash_messages;
    }

    // Aside handling — narrate with flavor but skip ALL state mutations.
    if ctx.aside {
        return handle_aside(ctx).await;
    }

    // Inline preprocessor (approach A): no separate Haiku call. The narrator/creature_smith
    // produces action_rewrite + action_flags in its JSON block. For prompt building, use
    // all-flags-on so no sections are gated out — the narrator has full context.
    let preprocessed = sidequest_game::PreprocessedAction {
        you: format!("You {}", ctx.action),
        named: format!("{} {}", ctx.char_name, ctx.action),
        intent: ctx.action.to_string(),
        is_power_grab: false,
        references_inventory: true,
        references_npc: true,
        references_ability: true,
        references_location: true,
    };
    let mut state_summary = prompt::build_prompt_context(ctx, &preprocessed).await;
    tracing::info!(
        raw = %ctx.action,
        "Prompt context built (preprocessor inlined into agent call)"
    );

    // Check if barrier mode is active (Structured/Cinematic turn mode).
    let barrier_combined_action: Option<String> = handle_barrier(ctx, &mut state_summary)
        .instrument(tracing::info_span!("turn.barrier", barrier_mode = tracing::field::Empty))
        .await;

    // Use combined action for barrier turns, original action for FreePlay
    let effective_action: std::borrow::Cow<str> = match &barrier_combined_action {
        Some(combined) => std::borrow::Cow::Borrowed(combined.as_str()),
        None => std::borrow::Cow::Borrowed(ctx.action),
    };

    // F9: Wish Consequence Engine — LLM-classified power-grab on clean input.
    if preprocessed.is_power_grab {
        let _wish_guard = tracing::info_span!("turn.preprocess.wish_check", is_power_grab = true).entered();
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

    let preprocess_done = std::time::Instant::now();

    // Build trope beat directives from previous turn's fired beats (if any)
    let trope_beat_directives = ctx.pending_trope_context.take();

    // Build active trope summary for background context (all agents)
    let active_trope_summary = {
        let active: Vec<_> = ctx
            .trope_states
            .iter()
            .filter(|ts| {
                matches!(
                    ts.status(),
                    sidequest_game::trope::TropeStatus::Active
                        | sidequest_game::trope::TropeStatus::Progressing
                )
            })
            .collect();
        if active.is_empty() {
            None
        } else {
            let lines: Vec<String> = active
                .iter()
                .map(|ts| {
                    let name = ctx
                        .trope_defs
                        .iter()
                        .find(|d| d.id.as_deref() == Some(ts.trope_definition_id()))
                        .map(|d| d.name.as_str())
                        .unwrap_or(ts.trope_definition_id());
                    format!(
                        "- {} [{:?}]: {:.0}% progressed",
                        name,
                        ts.status(),
                        ts.progression() * 100.0,
                    )
                })
                .collect();
            Some(format!(
                "[ACTIVE TROPES — BACKGROUND]\n{}",
                lines.join("\n")
            ))
        }
    };

    // Process the action through GameService (FreePlay mode — immediate resolution)
    let context = TurnContext {
        state_summary: Some(state_summary),
        in_combat: ctx.combat_state.in_combat(),
        in_chase: ctx.chase_state.is_some(),
        narrator_verbosity: ctx.narrator_verbosity,
        narrator_vocabulary: ctx.narrator_vocabulary,
        pending_trope_context: trope_beat_directives,
        active_trope_summary,
        genre: Some(ctx.genre_slug.to_string()),
        available_sfx: ctx.sfx_library.keys().cloned().collect(),
        // Story 15-16: merchant context injection
        npc_registry: ctx.npc_registry.clone(),
        npcs: ctx.snapshot.npcs.clone(),
        current_location: ctx.current_location.clone(),
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

    // Update preprocessed from inline agent output (approach A — no separate Haiku call).
    let _preprocessed = if let (Some(ref rw), Some(ref flags)) = (&result.action_rewrite, &result.action_flags) {
        tracing::info!(
            you = %rw.you, named = %rw.named, intent = %rw.intent,
            power_grab = flags.is_power_grab,
            "Inline preprocessor fields extracted from agent response"
        );
        sidequest_game::PreprocessedAction {
            you: rw.you.clone(),
            named: rw.named.clone(),
            intent: rw.intent.clone(),
            is_power_grab: flags.is_power_grab,
            references_inventory: flags.references_inventory,
            references_npc: flags.references_npc,
            references_ability: flags.references_ability,
            references_location: flags.references_location,
        }
    } else {
        tracing::debug!("Agent did not produce inline preprocessor fields — using defaults");
        preprocessed
    };

    // Watcher: narration generated (with intent classification and agent routing)
    WatcherEventBuilder::new("agent", WatcherEventType::AgentSpanClose)
        .field("narration_len", result.narration.len())
        .field("is_degraded", result.is_degraded)
        .field("turn_number", turn_number)
        .field_opt("classified_intent", &result.classified_intent)
        .field_opt("agent_routed_to", &result.agent_name)
        .field_opt("agent_duration_ms", &result.agent_duration_ms)
        .field_opt("token_count_in", &result.token_count_in)
        .field_opt("token_count_out", &result.token_count_out)
        .field("sfx_trigger_count", result.sfx_triggers.len())
        .field("has_new_npcs", result.npcs_present.iter().any(|n| n.is_new))
        .field("items_gained_count", result.items_gained.len())
        .send(ctx.state);

    // Watcher: prompt assembled breakdown (story 18-6 — Prompt Inspector tab)
    if let Some(ref zb) = result.zone_breakdown {
        let total_tokens: usize = zb.zones.iter().map(|z| z.total_tokens).sum();
        let section_count: usize = zb.zones.iter().map(|z| z.sections.len()).sum();
        WatcherEventBuilder::new("prompt", WatcherEventType::PromptAssembled)
            .field("turn_number", turn_number)
            .field_opt("agent", &result.agent_name)
            .field("total_tokens", total_tokens)
            .field("section_count", section_count)
            .field("zones", &zb.zones)
            .field("full_prompt", &zb.full_prompt)
            .send(ctx.state);
    }

    let agent_done = std::time::Instant::now();

    let mut messages = vec![];

    // Extract location header from narration (format: **Location Name**\n\n...)
    let state_update_span = tracing::info_span!(
        "turn.state_update",
        location_changed = tracing::field::Empty,
        items_gained = tracing::field::Empty,
    );
    let _state_update_guard = state_update_span.enter();

    let narration_text = &result.narration;
    if let Some(location) = extract_location_header(narration_text) {
        // Room-graph mode: validate + apply transition via canonical function (story 19-2).
        // Region mode (rooms empty): always valid — no room graph to check.
        let location_valid = if !ctx.rooms.is_empty() {
            match sidequest_game::room_movement::apply_validated_move(
                ctx.snapshot,
                &location,
                &ctx.rooms,
            ) {
                Ok(transition) => {
                    tracing::info!(
                        name: "room.transition",
                        from_room = %transition.from_room,
                        to_room = %transition.to_room,
                        exit_type = %transition.exit_type,
                    );
                    WatcherEventBuilder::new("room_graph", WatcherEventType::StateTransition)
                        .field("event", "room.transition")
                        .field("from_room", &transition.from_room)
                        .field("to_room", &transition.to_room)
                        .field("exit_type", &transition.exit_type)
                        .send(ctx.state);

                    // Story 19-5: Deplete active light source on room transition
                    if let Some(depleted_item) = ctx.inventory.deplete_light_on_transition() {
                        let item_name = depleted_item.name.as_str().to_owned();
                        tracing::info!(
                            name: "item.depleted",
                            item_name = %item_name,
                            item_id = %depleted_item.id.as_str(),
                            category = "light",
                        );
                        WatcherEventBuilder::new("inventory", WatcherEventType::StateTransition)
                            .field("event", "item.depleted")
                            .field("item_name", &item_name)
                            .field("item_id", depleted_item.id.as_str())
                            .field("category", "light")
                            .send(ctx.state);
                        messages.push(GameMessage::Narration {
                            payload: NarrationPayload {
                                text: format!("Your {} sputters and dies. The darkness closes in.", item_name),
                                state_delta: None,
                                footnotes: vec![],
                            },
                            player_id: ctx.player_id.to_string(),
                        });
                    }

                    true
                }
                Err(sidequest_game::room_movement::DispatchError::InvalidRoomTransition {
                    from_room,
                    to_room,
                    reason,
                }) => {
                    tracing::warn!(
                        name: "room.invalid_move",
                        attempted_room = %to_room,
                        current_room = %from_room,
                        reason = %reason,
                    );
                    WatcherEventBuilder::new("state", WatcherEventType::ValidationWarning)
                        .field("event", "room.invalid_move")
                        .field("attempted_room", &to_room)
                        .field("current_room", &from_room)
                        .field("reason", &reason)
                        .send(ctx.state);
                    false
                }
            }
        } else {
            true // Region mode — no validation
        };

        if location_valid {
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
            WatcherEventBuilder::new("state", WatcherEventType::StateTransition)
                .field("event", "location_changed")
                .field("location", &location)
                .field("turn_number", turn_number)
                .send(ctx.state);
            messages.push(GameMessage::ChapterMarker {
                payload: ChapterMarkerPayload {
                    title: Some(location.clone()),
                    location: Some(location.clone()),
                },
                player_id: ctx.player_id.to_string(),
            });
            let explored_locs: Vec<sidequest_protocol::ExploredLocation> = if !ctx.rooms.is_empty() {
                // Room-graph mode: use build_room_graph_explored for full room metadata
                sidequest_game::build_room_graph_explored(
                    &ctx.rooms,
                    &ctx.snapshot.discovered_rooms,
                    &ctx.snapshot.location,
                )
            } else {
                // Region mode: simple location list without room metadata
                ctx.discovered_regions
                    .iter()
                    .map(|name| sidequest_protocol::ExploredLocation {
                        name: name.clone(),
                        x: 0,
                        y: 0,
                        location_type: String::new(),
                        connections: vec![],
                        room_exits: vec![],
                        room_type: String::new(),
                        size: None,
                        is_current_room: false,
                    })
                    .collect()
            };
            messages.push(GameMessage::MapUpdate {
                payload: MapUpdatePayload {
                    current_location: location,
                    region: ctx.current_location.clone(),
                    explored: explored_locs,
                    fog_bounds: None,
                },
                player_id: ctx.player_id.to_string(),
            });
            ctx.turn_manager.advance_round();
            tracing::info!(
                new_round = ctx.turn_manager.round(),
                interaction = ctx.turn_manager.interaction(),
                "turn_manager.advance_round — location change"
            );
        }
    }

    let clean_narration = strip_location_header(narration_text)
        .replace("</s>", "")
        .replace("<|endoftext|>", "")
        .replace("<|end|>", "");

    // Accumulate narration history for context on subsequent turns.
    let truncated_narration: String = clean_narration.chars().take(300).collect();
    ctx.narration_history.push(format!(
        "[{}] Action: {}\nNarrator: {}",
        ctx.char_name, effective_action, truncated_narration
    ));
    if ctx.narration_history.len() > 20 {
        ctx.narration_history
            .drain(..ctx.narration_history.len() - 20);
    }

    // NPC registry + OCEAN personality shifts
    update_npc_registry(ctx, &result, &clean_narration);

    // Story 15-14: Enrich registry with structured NPC data (age, appearance, pronouns)
    // from GameSnapshot.npcs — update_npc_registry only gets regex-extracted data.
    {
        let before: Vec<(String, bool, bool, bool)> = ctx.npc_registry.iter().map(|e| {
            (e.name.clone(), e.pronouns.is_empty(), e.age.is_empty(), e.appearance.is_empty())
        }).collect();

        sidequest_game::enrich_registry_from_npcs(ctx.npc_registry, &ctx.snapshot.npcs);

        for (i, entry) in ctx.npc_registry.iter().enumerate() {
            if let Some((name, was_empty_pronouns, was_empty_age, was_empty_appearance)) = before.get(i) {
                let mut fields_added: u32 = 0;
                if *was_empty_pronouns && !entry.pronouns.is_empty() { fields_added += 1; }
                if *was_empty_age && !entry.age.is_empty() { fields_added += 1; }
                if *was_empty_appearance && !entry.appearance.is_empty() { fields_added += 1; }
                if fields_added > 0 {
                    WatcherEventBuilder::new("npc_registry", WatcherEventType::StateTransition)
                        .field("event", "npc.registry_enriched")
                        .field("npc_name", name)
                        .field("fields_added", fields_added)
                        .send(ctx.state);
                }
            }
        }
    }

    // Continuity validation — LLM-based (Haiku), runs via spawn_blocking.
    // Skip in combat — creature_smith output is structured, and the 18s Haiku call
    // doubles combat turn latency for marginal value.
    if !ctx.combat_state.in_combat() {
        validate_continuity(ctx, &clean_narration).await;
    } else {
        tracing::info!("Skipping continuity validation — in_combat, creature_smith output is structured");
    }

    let mutation_result =
        state_mutations::apply_state_mutations(ctx, &result, &clean_narration, &effective_action).await;
    let tier_events = mutation_result.tier_events;

    // Lore accumulation — wire accumulate_lore into post-narration dispatch (story 15-7, AC-1)
    if let Some(ref lore_entries) = result.lore_established {
        for entry in lore_entries {
            if entry.trim().is_empty() {
                continue;
            }
            match sidequest_game::accumulate_lore(
                ctx.lore_store,
                entry,
                sidequest_game::lore::LoreCategory::Event,
                turn_number as u64,
                std::collections::HashMap::new(),
            ) {
                Ok(fragment_id) => {
                    // AC-5: OTEL lore.fragment_accumulated
                    let category = "event";
                    let token_estimate = entry.len().div_ceil(4);
                    WatcherEventBuilder::new("lore", WatcherEventType::StateTransition)
                        .field("event", "lore.fragment_accumulated")
                        .field("fragment_id", &fragment_id)
                        .field("category", category)
                        .field("turn", turn_number)
                        .field("token_estimate", token_estimate)
                        .send(ctx.state);
                    tracing::info!(
                        fragment_id = %fragment_id,
                        category = category,
                        turn = turn_number,
                        token_estimate = token_estimate,
                        "lore.fragment_accumulated"
                    );

                    // Story 15-24: Persist lore fragment to SQLite for cross-session survival.
                    let persist_fragment = sidequest_game::LoreFragment::new(
                        fragment_id.clone(),
                        sidequest_game::lore::LoreCategory::Event,
                        entry.clone(),
                        sidequest_game::LoreSource::GameEvent,
                        Some(turn_number as u64),
                        std::collections::HashMap::new(),
                    );
                    match ctx.state.persistence().append_lore_fragment(
                        ctx.genre_slug,
                        ctx.world_slug,
                        ctx.player_name_for_save,
                        &persist_fragment,
                    ).await {
                        Ok(()) => {
                            WatcherEventBuilder::new("lore", WatcherEventType::StateTransition)
                                .field("event", "lore.fragment_persisted")
                                .field("fragment_id", &fragment_id)
                                .field("category", category)
                                .send(ctx.state);
                            tracing::info!(fragment_id = %fragment_id, "lore.fragment_persisted");
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, fragment_id = %fragment_id, "lore.fragment_persist_failed");
                        }
                    }

                    // AC-3: Call daemon embed() to generate embedding for the new fragment.
                    // AC-6: Emit lore.embedding_generated with fragment_id at call site.
                    let config = sidequest_daemon_client::DaemonConfig::default();
                    if let Ok(mut client) = sidequest_daemon_client::DaemonClient::connect(config).await {
                        let embed_params = sidequest_daemon_client::EmbedParams {
                            text: entry.clone(),
                        };
                        match client.embed(embed_params).await {
                            Ok(embed_result) => {
                                // Attach embedding to fragment in store
                                if let Err(e) = ctx.lore_store.set_embedding(&fragment_id, embed_result.embedding) {
                                    tracing::warn!(error = %e, fragment_id = %fragment_id, "lore.embedding_attach_failed");
                                } else {
                                    // AC-6: OTEL lore.embedding_generated
                                    WatcherEventBuilder::new("lore", WatcherEventType::StateTransition)
                                        .field("event", "lore.embedding_generated")
                                        .field("fragment_id", &fragment_id)
                                        .field("latency_ms", embed_result.latency_ms)
                                        .field("model", &embed_result.model)
                                        .send(ctx.state);
                                }
                            }
                            Err(e) => {
                                // Daemon unavailable — fragment stored without embedding.
                                // Semantic search degrades to keyword fallback. Not silent:
                                // we log it loudly.
                                tracing::warn!(
                                    error = %e,
                                    fragment_id = %fragment_id,
                                    "lore.embedding_generation_failed — fragment stored without embedding"
                                );
                            }
                        }
                    } else {
                        tracing::warn!(
                            fragment_id = %fragment_id,
                            "lore.daemon_connect_failed — fragment stored without embedding"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "lore.accumulate_failed");
                }
            }
        }
    }

    // Story 15-20: build narration state delta from current ctx locals via game-crate.
    // Patch a temp snapshot with current locals so build_protocol_delta reads fresh values.
    // Diff against before_snapshot (captured at dispatch entry) to detect what changed.
    let narration_state_delta = {
        let mut temp_state = ctx.snapshot.clone();
        temp_state.location =
            extract_location_header(narration_text).unwrap_or_else(|| ctx.current_location.clone());
        temp_state.quest_log = ctx.quest_log.clone();
        if let Some(ch) = temp_state.characters.first().cloned() {
            let mut updated = ch;
            updated.core.hp = *ctx.hp;
            updated.core.max_hp = *ctx.max_hp;
            updated.core.level = *ctx.level;
            updated.core.inventory = ctx.inventory.clone();
            temp_state.characters = vec![updated];
        }
        let snap_after = sidequest_game::delta::snapshot(&temp_state);
        let narration_delta = sidequest_game::delta::compute_delta(&before_snapshot, &snap_after);
        sidequest_game::build_protocol_delta(&narration_delta, &temp_state, &result.items_gained)
    };

    // Build response messages (narration, party status, inventory)
    build_response_messages(
        ctx,
        &clean_narration,
        narration_text,
        &result,
        &tier_events,
        &effective_action,
        &mut messages,
        narration_state_delta,
    )
    .await;

    drop(_state_update_guard);

    let system_tick_span = tracing::info_span!(
        "turn.system_tick",
        combat_changed = tracing::field::Empty,
        tropes_fired = tracing::field::Empty,
        achievements_earned = tracing::field::Empty,
    );
    let _system_tick_guard = system_tick_span.enter();

    let combat_active = ctx.combat_state.in_combat();
    combat::process_combat_and_chase(ctx, &clean_narration, &result, &mut messages, mutation_result.combat_just_ended, mutation_result.combat_just_started)
        .instrument(tracing::info_span!(
            "turn.system_tick.combat",
            in_combat = combat_active,
        ))
        .await;

    let (fired_beats, earned_achievements) = {
        // Initialize TropeState for each definition if empty.  Definitions
        // are loaded in dispatch_connect but TropeState instances were never
        // created from them — the tick ran on an empty vec every turn.
        if ctx.trope_states.is_empty() && !ctx.trope_defs.is_empty() {
            for def in ctx.trope_defs.iter() {
                let id = def.id.as_deref().unwrap_or(def.name.as_str());
                ctx.trope_states.push(sidequest_game::trope::TropeState::new(id));
            }
            tracing::info!(
                count = ctx.trope_states.len(),
                "trope_states.initialized — created from definitions (were empty)"
            );
        }
        let _tropes_guard = tracing::info_span!(
            "turn.system_tick.tropes",
            active_count = ctx.trope_states.len(),
        ).entered();
        tropes::process_tropes(ctx, &clean_narration, &mut messages)
    };
    system_tick_span.record("tropes_fired", fired_beats.len() as u64);
    system_tick_span.record("achievements_earned", earned_achievements.len() as u64);

    // Format beat context for NEXT turn's narrator prompt injection.
    // Beats fire after narration, so they inform the next turn — same as Python's
    // _pending_escalation_beats pattern.
    if !fired_beats.is_empty() {
        let _beat_ctx_guard = tracing::info_span!(
            "turn.system_tick.beat_context",
            beats_count = fired_beats.len(),
        ).entered();

        let mut troper = sidequest_agents::agents::troper::TroperAgent::new();
        troper.set_fired_beats(fired_beats);
        troper.set_trope_definitions(ctx.trope_defs.to_vec());
        troper.set_trope_states(ctx.trope_states.clone());
        *ctx.pending_trope_context = troper.build_beats_context();
    }

    drop(_system_tick_guard);

    let media_span = tracing::info_span!(
        "turn.media",
        render_enqueued = tracing::field::Empty,
        audio_cue_sent = tracing::field::Empty,
    );
    let _media_guard = media_span.enter();

    render::process_render(ctx, &clean_narration, narration_text, &result).await;

    let location_changed = *ctx.current_location != location_before_turn;
    audio::process_audio(ctx, &clean_narration, &mut messages, &result, location_changed, mutation_result.combat_just_ended).await;

    // Record this interaction in the turn manager
    ctx.turn_manager.record_interaction();
    tracing::info!(
        interaction = ctx.turn_manager.interaction(),
        round = ctx.turn_manager.round(),
        "turn_manager.record_interaction"
    );

    drop(_media_guard);

    // Sync scattered locals into the canonical snapshot, then persist (story 15-8)
    sync_locals_to_snapshot(ctx, narration_text);

    // Story 15-20: compute state delta and broadcast typed messages
    {
        let after_snapshot = sidequest_game::delta::snapshot(ctx.snapshot);
        let game_delta = sidequest_game::delta::compute_delta(&before_snapshot, &after_snapshot);

        // OTEL event: delta.computed (story 15-20 AC)
        let changed_count = [
            game_delta.characters_changed(),
            game_delta.npcs_changed(),
            game_delta.location_changed(),
            game_delta.quest_log_changed(),
            game_delta.combat_changed(),
            game_delta.chase_changed(),
            game_delta.atmosphere_changed(),
            game_delta.regions_changed(),
            game_delta.tropes_changed(),
        ]
        .iter()
        .filter(|&&b| b)
        .count();
        let snapshot_size_bytes = serde_json::to_string(ctx.snapshot)
            .map(|s| s.len())
            .unwrap_or(0);
        tracing::info!(
            changed_fields = changed_count,
            snapshot_size_bytes = snapshot_size_bytes,
            is_empty = game_delta.is_empty(),
            "delta.computed"
        );

        // Generate typed broadcast messages from the delta
        let broadcast_msgs =
            sidequest_game::broadcast_state_changes(&game_delta, ctx.snapshot);
        for msg in broadcast_msgs {
            let _ = ctx.tx.send(msg).await;
        }
    }

    persist_game_state(ctx, narration_text, &clean_narration).await;

    // TTS streaming
    spawn_tts_pipeline(ctx, &clean_narration, narration_text, &result);

    // GM Panel snapshot + timing telemetry
    emit_telemetry(ctx, turn_number, &result, turn_start, preprocess_done, agent_done);

    let char_class: String = ctx
        .character_json
        .as_ref()
        .and_then(|cj| cj.get("char_class"))
        .and_then(|c| c.as_str())
        .unwrap_or("Adventurer")
        .to_string();

    session_sync::sync_back_to_shared_session(
        ctx,
        &messages,
        &clean_narration,
        &char_class,
        &effective_action,
    )
    .await;

    messages
}

// ── Inline helpers extracted from dispatch_player_action ──────────────────

/// Handle turn barrier coordination for structured/cinematic multiplayer turns.
async fn handle_barrier(
    ctx: &mut DispatchContext<'_>,
    state_summary: &mut String,
) -> Option<String> {
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
                tracing::info!(player_id = %ctx.player_id, "barrier.submit — action submitted, waiting for other players");
                barrier.submit_action(ctx.player_id, ctx.action);

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

                let result = barrier_clone.wait_for_turn().await;
                tracing::info!(
                    timed_out = result.timed_out,
                    missing = ?result.missing_players,
                    genre = %ctx.genre_slug,
                    world = %ctx.world_slug,
                    "Turn barrier resolved"
                );

                let auto_resolved_names = result.auto_resolved_character_names();
                let auto_resolved_context = result.format_auto_resolved_context();

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

                let auto_ctx = if auto_resolved_context.is_empty() {
                    String::new()
                } else {
                    format!("\n{}\n", auto_resolved_context)
                };
                *state_summary = format!(
                    "Combined party actions:\n{}\n{}\nPERSPECTIVE: Write in third-person omniscient. Do NOT use 'you' for any character. Name all characters explicitly.\n\n{}",
                    combined, auto_ctx, state_summary
                );

                return Some(combined);
            }
        }
    }
    None
}

/// Update NPC registry from structured narrator output and apply OCEAN personality shifts.
fn update_npc_registry(
    ctx: &mut DispatchContext<'_>,
    result: &sidequest_agents::orchestrator::ActionResult,
    _clean_narration: &str,
) {
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
                entry.last_seen_turn = turn_approx;
                if !ctx.current_location.is_empty() {
                    entry.location = ctx.current_location.to_string();
                }
                if npc.name.len() > entry.name.len() {
                    entry.name = npc.name.clone();
                }
                // pronouns, role, appearance are NOT backfilled from narrator JSON.
                // enrich_registry_from_npcs() is the authoritative source (from Npc structs).
                // Narrator JSON extraction was a silent fallback — see CLAUDE.md "No Silent Fallbacks".
            } else {
                // Register ANY NPC not already in the registry, regardless of
                // is_new.  The LLM's is_new flag is advisory, not a gate —
                // defaulting to false via #[serde(default)] silently dropped
                // every NPC the LLM mentioned without the explicit flag.
                let span = tracing::info_span!(
                    "npc.registration",
                    npc_name = %npc.name,
                    npc_role = %npc.role,
                    ocean_summary = tracing::field::Empty,
                    archetype_source = tracing::field::Empty,
                    namegen_validated = tracing::field::Empty,
                    genre = %ctx.genre_slug,
                );
                let _guard = span.enter();

                // NPC GATE: Run sidequest-namegen server-side to generate an
                // authoritative identity. This validates the narrator used the tool
                // AND enriches the registry with culture/faction/archetype data.
                let namegen_result = ctx.state.namegen_binary_path().and_then(|binary| {
                    let output = std::process::Command::new(binary)
                        .arg("--genre-packs-path")
                        .arg(ctx.state.genre_packs_path())
                        .arg("--genre")
                        .arg(ctx.genre_slug)
                        .arg("--role")
                        .arg(if npc.role.is_empty() { "unknown" } else { &npc.role })
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .output()
                        .ok()?;
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        tracing::warn!(
                            error = %stderr,
                            "npc_gate.namegen_failed — falling back to narrator-provided identity"
                        );
                        return None;
                    }
                    serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()
                });

                let (ocean_profile, ocean_summary, source) = if let Some(ref gen) = namegen_result {
                    // Use the generated identity's OCEAN profile
                    let profile = gen.get("ocean").and_then(|o| {
                        Some(sidequest_genre::OceanProfile {
                            openness: o.get("openness")?.as_f64()?,
                            conscientiousness: o.get("conscientiousness")?.as_f64()?,
                            extraversion: o.get("extraversion")?.as_f64()?,
                            agreeableness: o.get("agreeableness")?.as_f64()?,
                            neuroticism: o.get("neuroticism")?.as_f64()?,
                        })
                    }).unwrap_or_else(sidequest_genre::OceanProfile::random);
                    let summary = gen.get("ocean_summary")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| profile.behavioral_summary());
                    let src = gen.get("archetype")
                        .and_then(|v| v.as_str())
                        .unwrap_or("namegen")
                        .to_string();
                    (profile, summary, src)
                } else {
                    // Fallback: random archetype OCEAN from genre pack
                    let loader = GenreLoader::new(vec![ctx.state.genre_packs_path().to_path_buf()]);
                    let from_pack = GenreCode::new(ctx.genre_slug).ok()
                        .and_then(|code| loader.load(&code).ok())
                        .and_then(|pack| {
                            let with_ocean: Vec<_> = pack.archetypes.iter()
                                .filter(|a| a.ocean.is_some()).collect();
                            if with_ocean.is_empty() { return None; }
                            use rand::prelude::IndexedRandom;
                            let arch = with_ocean.choose(&mut rand::rng())?;
                            let profile = arch.ocean.as_ref()?.with_jitter(1.5);
                            Some((profile, arch.name.as_str().to_string()))
                        })
                        .unwrap_or_else(|| (sidequest_genre::OceanProfile::random(), "random".to_string()));
                    let summary = from_pack.0.behavioral_summary();
                    (from_pack.0, summary, from_pack.1)
                };

                // Validation: log whether namegen was used
                let validated = namegen_result.is_some();
                span.record("namegen_validated", validated);
                span.record("ocean_summary", &ocean_summary.as_str());
                span.record("archetype_source", &source.as_str());

                if !validated && ctx.state.namegen_binary_path().is_some() {
                    tracing::warn!(
                        npc_name = %npc.name,
                        "npc_gate.validation_warning — namegen binary available but generation failed; narrator name accepted without tool verification"
                    );
                }

                tracing::info!(
                    name = %npc.name, pronouns = %npc.pronouns, role = %npc.role,
                    ocean = %ocean_summary, archetype = %source,
                    namegen_validated = validated,
                    "npc_registry.new — registered with {} identity",
                    if validated { "namegen-enriched" } else { "fallback" }
                );
                ctx.npc_registry.push(NpcRegistryEntry {
                    name: npc.name.clone(),
                    pronouns: npc.pronouns.clone(),
                    role: npc.role.clone(),
                    age: String::new(),
                    appearance: npc.appearance.clone(),
                    location: ctx.current_location.to_string(),
                    last_seen_turn: turn_approx,
                    ocean_summary: ocean_summary.clone(),
                    ocean: Some(ocean_profile),
                    hp: 0,
                    max_hp: 0,
                });
                WatcherEventBuilder::new("npc_registry", WatcherEventType::StateTransition)
                    .field("action", "npc_registered")
                    .field("name", &npc.name)
                    .field("role", &npc.role)
                    .field("ocean", &ocean_summary)
                    .field("namegen_validated", validated)
                    .field("archetype_source", &source)
                    .field("registry_size", ctx.npc_registry.len())
                    .send(ctx.state);
            }
        }
    }
    // Unconditional watcher event — GM panel sees the scan ran even if no new NPCs
    WatcherEventBuilder::new("npc_registry", WatcherEventType::SubsystemExerciseSummary)
        .field("event", "npc_registry.scan")
        .field("npcs_in_narration", result.npcs_present.len())
        .field("registry_size", ctx.npc_registry.len())
        .field("turn", ctx.turn_manager.interaction())
        .send(ctx.state);

    // OCEAN personality shifts — typed directly from narrator's structured JSON block.
    // No keyword matching. The narrator emits event_type as a typed enum variant.
    {
        let personality_events: Vec<(String, sidequest_game::PersonalityEvent)> = result
            .personality_events
            .iter()
            .map(|pe| (pe.npc.clone(), pe.event_type))
            .collect();

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
}

/// Continuity validation — LLM-based check of narrator output against game state.
///
/// Uses Haiku classification to detect contradictions rather than keyword matching.
/// Runs via spawn_blocking so it doesn't block the tokio runtime.
async fn validate_continuity(ctx: &mut DispatchContext<'_>, clean_narration: &str) {
    let dead_npcs: Vec<String> = ctx
        .npc_registry
        .iter()
        .filter(|n| n.max_hp > 0 && n.hp <= 0)
        .map(|n| n.name.clone())
        .collect();

    let inventory_items: Vec<String> = ctx
        .inventory
        .items
        .iter()
        .map(|i| i.name.as_str().to_string())
        .collect();

    let validation_result =
        sidequest_agents::continuity_validator::validate_continuity_llm_async(
            clean_narration,
            &ctx.current_location,
            &dead_npcs,
            &inventory_items,
            "", // time_of_day not tracked in dispatch context yet
        )
        .await;

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
        *ctx.continuity_corrections = corrections;
    }
}

/// Build narration, party status, inventory, and RAG messages.
///
/// Story 15-20: `narration_state_delta` is pre-built via `build_protocol_delta`
/// using game-crate delta computation instead of inline construction.
async fn build_response_messages(
    ctx: &mut DispatchContext<'_>,
    clean_narration: &str,
    _narration_text: &str,
    result: &sidequest_agents::orchestrator::ActionResult,
    tier_events: &[sidequest_game::AffinityTierUpEvent],
    _effective_action: &str,
    messages: &mut Vec<GameMessage>,
    narration_state_delta: sidequest_protocol::StateDelta,
) {
    // Merge narrator footnotes with affinity tier-up events
    let mut footnotes = result.footnotes.clone();
    for event in tier_events {
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

    // Send narration to client IMMEDIATELY — don't wait for state cleanup.
    // The user sees prose while we update game state in the background.
    // Story 15-20: state_delta built via game-crate delta path, not inline.
    let narration_msg = GameMessage::Narration {
        payload: NarrationPayload {
            text: clean_narration.to_string(),
            state_delta: Some(narration_state_delta),
            footnotes,
        },
        player_id: ctx.player_id.to_string(),
    };
    let _ = ctx.tx.send(narration_msg).await;
    tracing::info!("Narration sent to client — state cleanup continues async");

    // RAG pipeline: convert new footnotes to discovered facts
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

    let _ = ctx.tx.send(GameMessage::NarrationEnd {
        payload: NarrationEndPayload {
            state_delta: None,
        },
        player_id: ctx.player_id.to_string(),
    }).await;

    // Party status
    {
        let char_class: String = ctx
            .character_json
            .as_ref()
            .and_then(|cj| cj.get("char_class"))
            .and_then(|c| c.as_str())
            .unwrap_or("Adventurer")
            .to_string();

        let mut party_members = vec![PartyMember {
            player_id: ctx.player_id.to_string(),
            name: ctx.player_name_for_save.to_string(),
            character_name: ctx.char_name.to_string(),
            current_hp: *ctx.hp,
            max_hp: *ctx.max_hp,
            statuses: vec![],
            class: char_class,
            level: *ctx.level,
            portrait_url: None,
            current_location: ctx.current_location.clone(),
        }];
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            for (pid, ps) in &ss.players {
                if pid == ctx.player_id {
                    continue;
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

    // Inventory
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
}

/// Sync scattered DispatchContext locals into the canonical GameSnapshot.
///
/// Story 15-8: The dispatch pipeline still uses individual locals (ctx.hp,
/// ctx.inventory, etc.) throughout the turn. Before persisting, we sync those
/// locals into ctx.snapshot so persist_game_state() can save it directly
/// without loading from SQLite first.
fn sync_locals_to_snapshot(ctx: &mut DispatchContext<'_>, _narration_text: &str) {
    // Use ctx.current_location (authoritative after room-graph validation in story 19-2)
    // instead of re-extracting from narration text, which would bypass validation.
    ctx.snapshot.location = ctx.current_location.clone();
    ctx.snapshot.turn_manager = ctx.turn_manager.clone();
    ctx.snapshot.npc_registry = ctx.npc_registry.clone();
    ctx.snapshot.genie_wishes = ctx.genie_wishes.clone();
    ctx.snapshot.axis_values = ctx.axis_values.clone();
    ctx.snapshot.combat = ctx.combat_state.clone();
    ctx.snapshot.chase = ctx.chase_state.clone();
    // Sync StructuredEncounter from live combat/chase state
    ctx.snapshot.encounter = if ctx.combat_state.in_combat() {
        Some(sidequest_game::StructuredEncounter::from_combat_state(ctx.combat_state))
    } else if let Some(ref cs) = ctx.chase_state {
        Some(sidequest_game::StructuredEncounter::from_chase_state(cs))
    } else {
        None
    };
    ctx.snapshot.discovered_regions = ctx.discovered_regions.clone();
    ctx.snapshot.active_tropes = ctx.trope_states.clone();
    ctx.snapshot.achievement_tracker = ctx.achievement_tracker.clone();
    ctx.snapshot.quest_log = ctx.quest_log.clone();
    ctx.snapshot.resource_state = ctx.resource_state.clone();
    if let Some(ref cj) = ctx.character_json {
        if let Ok(ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
            if let Some(saved_ch) = ctx.snapshot.characters.first_mut() {
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
}

/// Persist game state — save the canonical snapshot directly (no load round-trip).
///
/// Story 15-8: The old implementation loaded from SQLite on every turn just to
/// merge scattered locals, then saved. Now ctx.snapshot is synced before this
/// call, so we save directly — one round-trip instead of two.
async fn persist_game_state(
    ctx: &mut DispatchContext<'_>,
    _narration_text: &str,
    clean_narration: &str,
) {
    if ctx.genre_slug.is_empty() || ctx.world_slug.is_empty() {
        tracing::debug!("persist_game_state skipped — empty genre or world slug");
        return;
    }

    // Append the current narration entry to ctx.snapshot and persist to narrative_log table
    let narrative_entry = sidequest_game::NarrativeEntry {
        timestamp: 0,
        round: ctx.turn_manager.interaction() as u32,
        author: "narrator".to_string(),
        content: clean_narration.to_string(),
        tags: vec![],
        encounter_tags: vec![],
        speaker: None,
        entry_type: None,
    };
    ctx.snapshot.narrative_log.push(narrative_entry.clone());

    // Write to append-only narrative_log table in SQLite
    if let Err(e) = ctx
        .state
        .persistence()
        .append_narrative(ctx.genre_slug, ctx.world_slug, ctx.player_name_for_save, &narrative_entry)
        .await
    {
        tracing::warn!(error = %e, "Failed to append narrative log entry");
    }

    // Emit encounter OTEL event if active
    if let Some(ref enc) = ctx.snapshot.encounter {
        WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
            .field("encounter_type", &enc.encounter_type)
            .field("beat", enc.beat)
            .field("metric_name", &enc.metric.name)
            .field("metric_current", enc.metric.current)
            .field("metric_threshold", enc.metric.threshold_high.or(enc.metric.threshold_low))
            .field("phase", enc.structured_phase.map(|p| format!("{:?}", p)))
            .field("resolved", enc.resolved)
            .field("actor_count", enc.actors.len())
            .field_opt("mood_override", &enc.mood_override)
            .field_opt("outcome", &enc.outcome)
            .send(ctx.state);
    }

    // Save ctx.snapshot directly — no load round-trip needed (story 15-8)
    let start = std::time::Instant::now();
    match ctx
        .state
        .persistence()
        .save(
            ctx.genre_slug,
            ctx.world_slug,
            ctx.player_name_for_save,
            &ctx.snapshot,
        )
        .await
    {
        Ok(_) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            tracing::info!(
                player = %ctx.player_name_for_save,
                turn = ctx.turn_manager.interaction(),
                location = %ctx.current_location,
                ctx.hp = *ctx.hp,
                items = ctx.inventory.items.len(),
                save_latency_ms = elapsed_ms,
                "session.saved — game state persisted"
            );
            // OTEL: persistence save latency for GM panel verification
            WatcherEventBuilder::new("persistence", WatcherEventType::SubsystemExerciseSummary)
                .field("save_latency_ms", elapsed_ms)
                .field("player", ctx.player_name_for_save)
                .field("turn", ctx.turn_manager.interaction())
                .send(ctx.state);

            // Also write to the dedicated narrative_log SQLite table
            // (enables recent_narrative() for "Previously On..." reconnect recaps)
            match ctx
                .state
                .persistence()
                .append_narrative(
                    ctx.genre_slug,
                    ctx.world_slug,
                    ctx.player_name_for_save,
                    &narrative_entry,
                )
                .await
            {
                Ok(()) => {
                    WatcherEventBuilder::new("persistence", WatcherEventType::SubsystemExerciseSummary)
                        .field("event", "persistence.narrative_appended")
                        .field("turn", ctx.turn_manager.interaction())
                        .field("length", clean_narration.len())
                        .field("player", ctx.player_name_for_save)
                        .send(ctx.state);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to append narrative to SQLite table");
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to persist game state");
            WatcherEventBuilder::new("persistence", WatcherEventType::ValidationWarning)
                .field("event", "persistence.save_failed")
                .field("error", &format!("{e}"))
                .field("player", ctx.player_name_for_save)
                .field("turn", ctx.turn_manager.interaction())
                .send(ctx.state);
        }
    }
}

/// Spawn TTS streaming pipeline as a background task.
fn spawn_tts_pipeline(
    ctx: &mut DispatchContext<'_>,
    clean_narration: &str,
    narration_text: &str,
    result: &sidequest_agents::orchestrator::ActionResult,
) {
    if clean_narration.is_empty() || ctx.state.tts_disabled() {
        return;
    }

    let segmenter = sidequest_game::SentenceSegmenter::new();
    let segments = segmenter.segment(clean_narration);
    tracing::info!(
        segment_count = segments.len(),
        narration_len = clean_narration.len(),
        "tts.segmented"
    );
    if segments.is_empty() {
        return;
    }

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

    {
        let first_preview = tts_segments.first().map(|f| f.text.chars().take(80).collect::<String>());
        WatcherEventBuilder::new("tts", WatcherEventType::AgentSpanOpen)
            .field("segment_count", tts_segments.len())
            .field("total_chars", tts_segments.iter().map(|s| s.text.len()).sum::<usize>())
            .field_opt("first_segment", &first_preview)
            .send(ctx.state);
    }

    let player_id_for_tts = ctx.player_id.to_string();
    let state_for_tts = ctx.state.clone();
    let ss_holder_for_tts = ctx.shared_session_holder.clone();
    // Clone the direct mpsc sender so NARRATION_CHUNK goes through the same
    // ordered channel as NARRATION — guaranteeing chunks arrive at the client
    // BEFORE their corresponding binary audio frames (which go via broadcast).
    let tx_for_tts = ctx.tx.clone();
    let tts_config = sidequest_game::tts_stream::TtsStreamConfig::default();
    let streamer = sidequest_game::tts_stream::TtsStreamer::new(tts_config);

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

        let daemon_config = sidequest_daemon_client::DaemonConfig::default();
        let synthesizer = match sidequest_daemon_client::DaemonClient::connect(daemon_config).await
        {
            Ok(client) => DaemonSynthesizer {
                client: tokio::sync::Mutex::new(client),
            },
            Err(e) => {
                tracing::warn!(error = %e, "TTS daemon unavailable — skipping voice synthesis");
                return;
            }
        };

        let stream_handle = tokio::spawn(async move {
            if let Err(e) = streamer.stream(tts_segments, &synthesizer, msg_tx).await {
                tracing::warn!(error = %e, "TTS stream failed");
            }
        });

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

        while let Some(tts_msg) = msg_rx.recv().await {
            match tts_msg {
                sidequest_game::tts_stream::TtsMessage::Start { total_segments } => {
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
                                            "",
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
                    // Send NARRATION_CHUNK via direct mpsc (same channel as NARRATION)
                    // so the client receives text BEFORE the binary audio frame that
                    // follows.  The old path (send_to_acting_player) spawned a task
                    // with double-mutex locking, causing binary audio to consistently
                    // arrive first and the client to discard it (no chunk to reveal).
                    if let Some(seg) = tts_segments_for_prerender.get(chunk.segment_index) {
                        let chunk_msg = GameMessage::NarrationChunk {
                            payload: sidequest_protocol::NarrationChunkPayload {
                                text: seg.text.clone(),
                            },
                            player_id: player_id_for_tts.clone(),
                        };
                        let _ = tx_for_tts.send(chunk_msg).await;
                    }

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

/// Emit GM panel snapshot and turn timing telemetry.
fn emit_telemetry(
    ctx: &mut DispatchContext<'_>,
    turn_number: u64,
    result: &sidequest_agents::orchestrator::ActionResult,
    turn_start: std::time::Instant,
    preprocess_done: std::time::Instant,
    agent_done: std::time::Instant,
) {
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
        WatcherEventBuilder::new("game", WatcherEventType::GameStateSnapshot)
            .field("turn_number", turn_approx)
            .field("snapshot", &snapshot)
            .send(ctx.state);
    }

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

    {
        let mut builder = WatcherEventBuilder::new("game", WatcherEventType::TurnComplete)
            .field("turn_id", turn_number)
            .field("turn_number", turn_number)
            .field("player_input", ctx.action)
            .field_opt("classified_intent", &result.classified_intent)
            .field_opt("agent_name", &result.agent_name)
            .field("agent_duration_ms", agent_ms)
            .field("is_degraded", result.is_degraded)
            .field("player_id", ctx.player_id)
            .field_opt("token_count_in", &result.token_count_in)
            .field_opt("token_count_out", &result.token_count_out)
            .field("spans", &spans)
            .field("total_duration_ms", total_ms);
        if result.is_degraded {
            builder = builder.severity(Severity::Warn);
        }
        builder.send(ctx.state);
    }
}

/// Handle an aside — out-of-character commentary that does not affect the game world.
///
/// Calls the narrator with an aside-specific prompt injection, then returns narration
/// only. Skips ALL state mutation subsystems: no combat, no chase, no tropes, no
/// renders, no music, no NPC registry, no narration history, no turn barrier.
async fn handle_aside(ctx: &mut DispatchContext<'_>) -> Vec<GameMessage> {
    tracing::info!(player = %ctx.char_name, action = %ctx.action, "aside — out-of-character, skipping state mutations");

    // Asides are out-of-character — no game state references, minimal prompt
    let aside_relevance = sidequest_game::PreprocessedAction {
        you: ctx.action.to_string(),
        named: ctx.action.to_string(),
        intent: ctx.action.to_string(),
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: false,
    };
    let mut state_summary = prompt::build_prompt_context(ctx, &aside_relevance).await;
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
        pending_trope_context: None,
        active_trope_summary: None,
        genre: Some(ctx.genre_slug.to_string()),
        available_sfx: ctx.sfx_library.keys().cloned().collect(),
        // Aside turns don't need merchant context — no state changes allowed
        npc_registry: Vec::new(),
        npcs: Vec::new(),
        current_location: String::new(),
    };
    let result = ctx
        .state
        .game_service()
        .process_action(&format!("(aside) {}", ctx.action), &context);

    // Watcher: prompt assembled for aside (story 18-6)
    if let Some(ref zb) = result.zone_breakdown {
        let total_tokens: usize = zb.zones.iter().map(|z| z.total_tokens).sum();
        let section_count: usize = zb.zones.iter().map(|z| z.sections.len()).sum();
        WatcherEventBuilder::new("prompt", WatcherEventType::PromptAssembled)
            .field("agent", "narrator")
            .field("total_tokens", total_tokens)
            .field("section_count", section_count)
            .field("zones", &zb.zones)
            .field("full_prompt", &zb.full_prompt)
            .send(ctx.state);
    }

    let narration_text = strip_location_header(&result.narration);

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

#[cfg(test)]
mod tests {
    /// Story 15-14: Verify the production dispatch pipeline actually calls
    /// enrich_registry_from_npcs after update_npc_registry(). Source-level grep
    /// of non-test code — strips the test module to avoid self-referential matches.
    #[test]
    fn dispatch_pipeline_calls_enrich_registry() {
        let source = include_str!("mod.rs");
        let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(
            production_code.contains("enrich_registry_from_npcs("),
            "enrich_registry_from_npcs() must be called in dispatch pipeline \
             (production code, not just tests) after update_npc_registry() — story 15-14"
        );
    }

    /// Story 15-14: Verify OTEL event npc.registry_enriched is emitted in production code
    /// so the GM panel can confirm enrichment is running.
    #[test]
    fn dispatch_pipeline_emits_registry_enriched_otel() {
        let source = include_str!("mod.rs");
        let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(
            production_code.contains("npc.registry_enriched")
                || production_code.contains("npc_registry_enriched"),
            "dispatch must emit npc.registry_enriched OTEL event so GM panel \
             can verify enrichment is running — story 15-14"
        );
    }
}

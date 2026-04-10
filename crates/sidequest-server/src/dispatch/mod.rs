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

mod aside;
mod audio;
mod barrier;
mod beat;
pub(crate) mod catch_up;
pub(crate) mod connect;
mod lore_sync;
mod npc_registry;
mod patching;
mod persistence;
pub(crate) mod pregen;
mod prompt;
mod render;
mod response;
mod session_sync;
mod slash;
mod state_mutations;
mod telemetry;
mod tropes;

use std::collections::HashMap;
use std::sync::Arc;

use tracing::Instrument;

use sidequest_agents::orchestrator::TurnContext;
use sidequest_protocol::{
    ChapterMarkerPayload, GameMessage, ItemDepletedPayload, MapUpdatePayload, NarrationEndPayload,
    NarrationPayload, ThinkingPayload,
};

use crate::extraction::{
    extract_location_header, strip_combat_brackets, strip_fenced_blocks, strip_fourth_wall,
    strip_location_header,
};
use crate::{shared_session, AppState, NpcRegistryEntry, WatcherEventBuilder, WatcherEventType};

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
    /// Confrontation type definitions from genre pack rules.yaml (story 28-1).
    /// Used by apply_beat(), format_encounter_context(), and beat population.
    pub confrontation_defs: Vec<sidequest_genre::ConfrontationDef>,
    pub aside: bool,
    /// Opening scenario directive — injected into Early zone on turn 0 only, then consumed.
    pub opening_directive: Option<String>,
    /// SFX library from genre pack: ID → list of file paths.
    pub sfx_library: std::collections::HashMap<String, Vec<String>>,
    /// Room definitions for room_graph navigation mode (from cartography.rooms).
    /// Empty for region-based navigation.
    pub rooms: Vec<sidequest_genre::RoomDef>,
    /// Hierarchical world graph for lore filtering (story 23-4).
    /// Populated from cartography.world_graph when navigation_mode is Hierarchical.
    pub world_graph: Option<sidequest_genre::WorldGraph>,
    /// Cartography metadata for MAP_UPDATE payloads (story 26-10).
    /// Pre-computed from genre pack CartographyConfig on session init.
    pub cartography_metadata: Option<sidequest_protocol::CartographyMetadata>,
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
    /// Monster Manual — persistent pre-generated content pool (ADR-059).
    /// Loaded from `~/.sidequest/manuals/{genre}_{world}.json` on session start.
    /// NPCs and encounters injected into game_state for narrator to reference.
    pub monster_manual: &'a mut sidequest_game::monster_manual::MonsterManual,
    /// Morpheme glossaries from genre pack conlang definitions (story 15-19).
    /// Used to detect conlang vocabulary in narration text and record to lore store.
    pub morpheme_glossaries: Vec<sidequest_game::MorphemeGlossary>,
    /// Name banks from genre pack conlang definitions (story 15-19).
    /// Injected into narrator prompt context for name consistency.
    pub name_banks: Vec<sidequest_game::NameBank>,
    /// Inventory carry mode from genre pack (Count or Weight). Story 19-7.
    pub carry_mode: sidequest_game::inventory::CarryMode,
    /// Weight limit when carry_mode is Weight. Story 19-7.
    pub weight_limit: Option<f64>,
}

impl<'a> DispatchContext<'a> {
    /// Add an item respecting the genre pack's carry mode (story 19-7).
    /// In Count mode, uses the hardcoded carry limit (50).
    /// In Weight mode, checks against weight_limit from InventoryPhilosophy.
    /// Add an item respecting the genre pack's carry mode (story 19-7).
    /// In Count mode, uses the hardcoded carry limit (50).
    /// In Weight mode, checks against weight_limit from InventoryPhilosophy.
    pub fn add_item(
        &mut self,
        item: sidequest_game::Item,
    ) -> Result<(), sidequest_game::inventory::InventoryError> {
        let result = match self.carry_mode {
            sidequest_game::inventory::CarryMode::Count => self.inventory.add(item, 50),
            sidequest_game::inventory::CarryMode::Weight => {
                let limit = self.weight_limit.unwrap_or(f64::INFINITY);
                self.inventory.add_weighted(item, limit)
            }
            _ => {
                // #[non_exhaustive] future variants — fall back to count-based
                tracing::warn!(carry_mode = ?self.carry_mode, "Unknown carry mode, falling back to count-based");
                self.inventory.add(item, 50)
            }
        };
        if let Err(ref e) = result {
            if matches!(
                e,
                sidequest_game::inventory::InventoryError::Overweight { .. }
            ) {
                WatcherEventBuilder::new("inventory", WatcherEventType::StateTransition)
                    .field("event", "item_rejected_overweight")
                    .field("total_weight", self.inventory.total_weight())
                    .field("weight_limit", self.weight_limit.unwrap_or(0.0))
                    .field("error", format!("{e}"))
                    .send();
            }
        }
        result
    }

    /// Whether any encounter is active (not resolved).
    /// Story 28-9: replaces `combat_state.in_combat() || chase_state.is_some()`.
    pub fn in_encounter(&self) -> bool {
        self.snapshot
            .encounter
            .as_ref()
            .map_or(false, |e| !e.resolved)
    }

    /// Whether a combat-type encounter is active.
    /// Story 28-9: replaces `combat_state.in_combat()`.
    pub fn in_combat(&self) -> bool {
        self.snapshot
            .encounter
            .as_ref()
            .map_or(false, |e| !e.resolved && e.encounter_type == "combat")
    }

    /// Whether a chase-type encounter is active.
    /// Story 28-9: replaces `chase_state.is_some()`.
    pub fn in_chase(&self) -> bool {
        self.snapshot
            .encounter
            .as_ref()
            .map_or(false, |e| !e.resolved && e.encounter_type == "chase")
    }
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
                ctx.character_json,
            );
            let pc = ss.player_count();
            if pc > 1 {
                WatcherEventBuilder::new("multiplayer", WatcherEventType::AgentSpanOpen)
                    .field("event", "multiplayer_action")
                    .field(
                        "session_key",
                        format!("{}:{}", ctx.genre_slug, ctx.world_slug),
                    )
                    .field("player_id", ctx.player_id)
                    .field("party_size", pc)
                    .send();
            }
        }
    }

    // Story 15-20: capture pre-turn snapshot for delta computation
    let before_snapshot = sidequest_game::delta::snapshot(ctx.snapshot);

    // ADR-073: capture full GameSnapshot before state mutations for TurnRecord.
    let before_game_snapshot = ctx.snapshot.clone();

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
        .send();

    // Unified model: no "active" broadcast. The barrier sends "submitted"
    // per-player when they seal their letter. The UI uses canType state
    // (locked on submit, unlocked on narration). No sequential turn-taking.

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

    // Two-pass inventory extraction: classify the PREVIOUS turn's narration for
    // item state transitions (consumed, sold, given, lost, destroyed). Runs before
    // the narrator LLM call so mutations are visible in the current turn's prompt.
    if let Some(prev_entry) = ctx.narration_history.last() {
        let carried_names: Vec<String> = ctx
            .inventory
            .carried()
            .map(|i| i.name.as_str().to_string())
            .collect();
        // FIX: removed carried_names.is_empty() guard that blocked first-item acquisition
        {
            // Split the history entry: "[CharName] Action: ...\nNarrator: ..."
            let (prev_action, prev_narration) = prev_entry
                .split_once("\nNarrator: ")
                .map(|(a, n)| {
                    let action = a.split_once("Action: ").map(|(_, act)| act).unwrap_or(a);
                    (action.to_string(), n.to_string())
                })
                .unwrap_or_default();

            if !prev_narration.is_empty() {
                let mutations =
                    sidequest_agents::inventory_extractor::extract_inventory_mutations_async(
                        &prev_action,
                        &prev_narration,
                        &carried_names,
                    )
                    .await;

                use sidequest_agents::inventory_extractor::MutationAction;

                for mutation in &mutations {
                    if mutation.action == MutationAction::Acquired {
                        // New item acquisition — add to inventory
                        if let Some(gold) = mutation.gold {
                            ctx.inventory.gold += gold;
                            tracing::info!(
                                gold_gained = gold,
                                total_gold = ctx.inventory.gold,
                                detail = %mutation.detail,
                                "inventory.two_pass_gold_acquired"
                            );
                            WatcherEventBuilder::new(
                                "inventory",
                                WatcherEventType::StateTransition,
                            )
                            .field("action", "gold_acquired")
                            .field("gold_gained", gold)
                            .field("total_gold", ctx.inventory.gold)
                            .field("detail", &mutation.detail)
                            .send();
                        } else {
                            let item_id = mutation
                                .item_name
                                .to_lowercase()
                                .replace(' ', "_")
                                .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
                            // Skip if already in inventory
                            if ctx.inventory.find(&item_id).is_some() {
                                continue;
                            }
                            let category = mutation.category.as_deref().unwrap_or("misc");
                            if let (Ok(id), Ok(name), Ok(desc), Ok(cat), Ok(rarity)) = (
                                sidequest_protocol::NonBlankString::new(&item_id),
                                sidequest_protocol::NonBlankString::new(&mutation.item_name),
                                sidequest_protocol::NonBlankString::new(&mutation.detail),
                                sidequest_protocol::NonBlankString::new(category),
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
                                    uses_remaining: None,
                                    state: sidequest_game::ItemState::Carried,
                                };
                                let _ = ctx.add_item(item);
                                tracing::info!(
                                    item_name = %mutation.item_name,
                                    category = %category,
                                    "inventory.two_pass_item_acquired"
                                );
                                WatcherEventBuilder::new(
                                    "inventory",
                                    WatcherEventType::StateTransition,
                                )
                                .field("action", "item_acquired")
                                .field("item_name", &mutation.item_name)
                                .field("category", category)
                                .field("carried_count", ctx.inventory.item_count())
                                .send();
                            }
                        }
                        continue;
                    }

                    // Gold loss (spent, tossed, given away, etc.) — non-Acquired mutations
                    if let Some(gold) = mutation.gold {
                        if gold > 0 {
                            let actual = ctx.inventory.spend_gold(gold);
                            if actual < gold {
                                tracing::warn!(
                                    requested = gold,
                                    actual_spent = actual,
                                    remaining = ctx.inventory.gold,
                                    detail = %mutation.detail,
                                    "inventory.insufficient_gold — clamped to available balance"
                                );
                            }
                            tracing::info!(
                                gold_spent = actual,
                                total_gold = ctx.inventory.gold,
                                action = %mutation.action,
                                detail = %mutation.detail,
                                "inventory.two_pass_gold_spent"
                            );
                            WatcherEventBuilder::new(
                                "inventory",
                                WatcherEventType::StateTransition,
                            )
                            .field("action", "gold_spent")
                            .field("gold_spent", actual)
                            .field("total_gold", ctx.inventory.gold)
                            .field("mutation_action", format!("{}", mutation.action))
                            .field("detail", &mutation.detail)
                            .send();
                            continue;
                        }
                    }

                    // State transition on existing item
                    let item_lower = mutation.item_name.to_lowercase();
                    let matched_id = ctx
                        .inventory
                        .carried()
                        .find(|i| i.name.as_str().to_lowercase() == item_lower)
                        .map(|i| i.id.as_str().to_string());

                    if let Some(item_id) = matched_id {
                        let new_state = match &mutation.action {
                            MutationAction::Consumed => sidequest_game::ItemState::Consumed,
                            MutationAction::Sold => sidequest_game::ItemState::Sold {
                                to: mutation.detail.clone(),
                            },
                            MutationAction::Given => sidequest_game::ItemState::Given {
                                to: mutation.detail.clone(),
                            },
                            MutationAction::Lost => sidequest_game::ItemState::Lost {
                                reason: mutation.detail.clone(),
                            },
                            MutationAction::Destroyed => sidequest_game::ItemState::Destroyed {
                                reason: mutation.detail.clone(),
                            },
                            MutationAction::Acquired => unreachable!(),
                        };
                        match ctx.inventory.transition(&item_id, new_state) {
                            Ok(old_state) => {
                                tracing::info!(
                                    item_name = %mutation.item_name,
                                    old_state = %old_state,
                                    new_state = %mutation.action,
                                    detail = %mutation.detail,
                                    "inventory.two_pass_transition"
                                );
                                WatcherEventBuilder::new(
                                    "inventory",
                                    WatcherEventType::StateTransition,
                                )
                                .field("action", "two_pass_transition")
                                .field("item_name", &mutation.item_name)
                                .field("new_state", format!("{:?}", mutation.action))
                                .field("detail", &mutation.detail)
                                .field("carried_count", ctx.inventory.item_count())
                                .send();
                            }
                            Err(e) => {
                                tracing::warn!(
                                    item_name = %mutation.item_name,
                                    error = %e,
                                    "inventory.two_pass_transition_failed"
                                );
                            }
                        }
                    } else {
                        tracing::debug!(
                            item_name = %mutation.item_name,
                            "inventory.two_pass_no_match — item not found in carried inventory"
                        );
                    }
                }
            }
        }
    }

    // Slash command interception — route /commands to mechanical handlers, not the LLM.
    if let Some(slash_messages) = slash::handle_slash_command(ctx) {
        return slash_messages;
    }

    // Aside handling — narrate with flavor but skip ALL state mutations.
    if ctx.aside {
        return aside::handle_aside(ctx).await;
    }

    // Inline preprocessor (approach A): no separate Haiku call. The narrator/creature_smith
    // produces action_rewrite + action_flags in its JSON block. For prompt building, use
    // all-flags-on so no sections are gated out — the narrator has full context.
    let preprocessed = sidequest_game::PreprocessedAction {
        you: {
            let trimmed = ctx.action.trim_start();
            if trimmed.starts_with("I ") || trimmed.starts_with("I'") {
                // Already first-person — convert to second-person by replacing leading "I"
                format!("You {}", &trimmed[2..])
            } else if trimmed.starts_with("you ") || trimmed.starts_with("You ") {
                trimmed.to_string()
            } else {
                format!("You {}", ctx.action)
            }
        },
        named: format!("{} {}", ctx.char_name, ctx.action),
        intent: ctx.action.to_string(),
        is_power_grab: false,
        references_inventory: true,
        references_npc: true,
        references_ability: true,
        references_location: true,
    };
    // Sync StructuredEncounter before prompt context so format_encounter_context() can use it
    // encounter sync bridge removed in story 28-9 — encounter is maintained directly via apply_beat().

    let mut state_summary = prompt::build_prompt_context(ctx, &preprocessed).await;

    // Monster Manual: inject pre-generated NPCs and encounters into game_state (ADR-059)
    {
        let nearby = ctx.monster_manual.format_nearby_npcs(ctx.current_location);
        let creatures = ctx.monster_manual.format_area_creatures(ctx.in_combat());
        if !nearby.is_empty() {
            state_summary.push_str("\n\n");
            state_summary.push_str(&nearby);
            state_summary.push_str("\nNPC NAMING RULE: Use ONLY NPC names from the list above. Do NOT invent new character names. If you need an unnamed NPC, describe them by role or appearance (\"the blacksmith\", \"a weathered rider\") instead of giving them a name not on this list.");
        }
        if !creatures.is_empty() {
            state_summary.push_str("\n\n");
            state_summary.push_str(&creatures);
        }
        let npcs_injected = if nearby.is_empty() {
            0
        } else {
            nearby.lines().count()
        };
        let creatures_injected = if creatures.is_empty() {
            0
        } else {
            creatures.lines().count()
        };
        let _mm_span = tracing::info_span!(
            "monster_manual.injected",
            available_npcs = ctx.monster_manual.available_npcs().len(),
            available_encounters = ctx.monster_manual.available_encounters().len(),
            total_npcs = ctx.monster_manual.npcs.len(),
            total_encounters = ctx.monster_manual.encounters.len(),
            npcs_injected = npcs_injected,
            creatures_injected = creatures_injected,
            in_combat = ctx.in_combat(),
            location = %ctx.current_location,
        )
        .entered();
        tracing::info!("Monster Manual content injected (location-filtered)");
    }

    // Story 7-9: Scenario between-turn processing — gossip, NPC actions, clue activation.
    // Unified under "scenario" OTEL namespace for GM panel filtering.
    if let Some(ref mut scenario) = ctx.snapshot.scenario_state {
        if !scenario.is_resolved() {
            let _advance_span = tracing::info_span!("scenario.advance",
                turn = turn_number,
                tension = %format!("{:.2}", scenario.tension()),
            )
            .entered();

            let turn_number_u64 = turn_number as u64;
            let events = scenario.process_between_turns(&mut ctx.snapshot.npcs, turn_number_u64);

            let mut npc_action_lines: Vec<String> = Vec::new();
            for event in &events {
                match &event.event_type {
                    sidequest_game::ScenarioEventType::NpcAction { npc_name, action } => {
                        WatcherEventBuilder::new("scenario", WatcherEventType::StateTransition)
                            .field("event", "scenario.npc_action")
                            .field("npc_name", npc_name)
                            .field("action", format!("{:?}", action))
                            .field("turn", turn_number)
                            .field("tension", format!("{:.2}", scenario.tension()))
                            .send();
                        npc_action_lines.push(event.description.clone());
                    }
                    sidequest_game::ScenarioEventType::GossipSpread {
                        claims_spread,
                        contradictions_found,
                    } => {
                        WatcherEventBuilder::new("scenario", WatcherEventType::StateTransition)
                            .field("event", "scenario.gossip_spread")
                            .field("claims_spread", *claims_spread)
                            .field("contradictions_found", *contradictions_found)
                            .field("turn", turn_number)
                            .send();
                        npc_action_lines.push(event.description.clone());
                    }
                    sidequest_game::ScenarioEventType::ClueDiscovered { clue_id } => {
                        WatcherEventBuilder::new("scenario", WatcherEventType::StateTransition)
                            .field("event", "scenario.clue_discovered")
                            .field("clue_id", clue_id)
                            .field("turn", turn_number)
                            .send();
                    }
                    _ => {}
                }
            }

            if !npc_action_lines.is_empty() {
                state_summary.push_str("\n\n[NPC AUTONOMOUS ACTIONS THIS TURN]\n");
                state_summary.push_str("The following NPC actions happened between turns. Weave these into your narration:\n");
                for line in &npc_action_lines {
                    state_summary.push_str(&format!("- {}\n", line));
                }

                WatcherEventBuilder::new("scenario", WatcherEventType::SubsystemExerciseSummary)
                    .field("event", "scenario.npc_actions_injected")
                    .field("action_count", npc_action_lines.len())
                    .field("tension", format!("{:.2}", scenario.tension()))
                    .field("turn", turn_number)
                    .send();
            }
        }
    }

    // Scenario pressure events and scene budget — check SharedGameSession
    {
        let holder = ctx.shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let mut ss = ss_arc.lock().await;
            if let Some(ref scenario_pack) = ss.active_scenario {
                // Check for pressure events at this scene count
                for pressure_event in &scenario_pack.pacing.pressure_events {
                    if pressure_event.at_scene == ss.scene_count {
                        state_summary.push_str(&format!(
                            "\n\n[PRESSURE EVENT] {}\nWeave this event into the narration naturally.\n",
                            pressure_event.event,
                        ));
                        WatcherEventBuilder::new("scenario", WatcherEventType::StateTransition)
                            .field("event", "pressure_event_triggered")
                            .field("at_scene", pressure_event.at_scene)
                            .field("description", pressure_event.event.as_str())
                            .send();
                    }
                }
                // Check escalation beats by trope progression
                if let Some(ref mut scenario) = ctx.snapshot.scenario_state {
                    let progress = scenario.tension() as f64;
                    for beat in &scenario_pack.pacing.escalation_beats {
                        if (progress - beat.at).abs() < 0.05 {
                            state_summary.push_str(&format!("\n[ESCALATION] {}\n", beat.inject,));
                            WatcherEventBuilder::new("scenario", WatcherEventType::StateTransition)
                                .field("event", "escalation_beat")
                                .field("at_threshold", format!("{:.2}", beat.at))
                                .field("inject", beat.inject.as_str())
                                .send();
                        }
                    }
                }
                // Scene budget warning
                let budget = scenario_pack.pacing.scene_budget;
                if ss.scene_count >= budget.saturating_sub(2) && ss.scene_count < budget {
                    state_summary.push_str(
                        "\n[PACING] The scenario is nearing its conclusion. Begin steering toward resolution.\n",
                    );
                } else if ss.scene_count >= budget {
                    state_summary.push_str(
                        "\n[PACING] The scenario has exceeded its scene budget. Push hard toward a climactic resolution.\n",
                    );
                }
                ss.scene_count += 1;
            }
        }
    }

    tracing::info!(
        raw = %ctx.action,
        "Prompt context built (preprocessor inlined into agent call)"
    );

    // Check if barrier mode is active (Structured/Cinematic turn mode).
    let barrier_outcome: Option<barrier::BarrierOutcome> =
        barrier::handle_barrier(ctx, &mut state_summary)
            .instrument(tracing::info_span!(
                "turn.barrier",
                barrier_mode = tracing::field::Empty
            ))
            .await;

    // Non-claiming handlers skip the narrator — retrieve shared narration instead.
    // Only the claiming handler runs the expensive LLM call; others poll for the result.
    if let Some(ref outcome) = barrier_outcome {
        if !outcome.claimed_resolution {
            for attempt in 0..100u32 {
                if attempt > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                if let Some(narration) = outcome.barrier.get_resolution_narration() {
                    tracing::info!(
                        attempts = attempt + 1,
                        "barrier.non_claimer — retrieved shared narration, skipping narrator call"
                    );
                    let msg = GameMessage::Narration {
                        payload: NarrationPayload {
                            text: narration,
                            state_delta: None,
                            footnotes: vec![],
                        },
                        player_id: ctx.player_id.to_string(),
                    };
                    let _ = ctx.tx.send(msg).await;
                    let end = GameMessage::NarrationEnd {
                        payload: NarrationEndPayload { state_delta: None },
                        player_id: ctx.player_id.to_string(),
                    };
                    let _ = ctx.tx.send(end).await;
                    return vec![];
                }
            }
            tracing::warn!(
                "barrier.non_claimer — timed out (10s) waiting for shared narration, falling through to narrator"
            );
        }
    }

    // Use combined action for barrier turns, original action for FreePlay
    let effective_action: std::borrow::Cow<str> = match &barrier_outcome {
        Some(outcome) => std::borrow::Cow::Borrowed(outcome.combined_action.as_str()),
        None => std::borrow::Cow::Borrowed(ctx.action),
    };

    // F9: Wish Consequence Engine — LLM-classified power-grab on clean input.
    if preprocessed.is_power_grab {
        let _wish_guard =
            tracing::info_span!("turn.preprocess.wish_check", is_power_grab = true).entered();
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

    // Recalculate maturity from current turn count — the snapshot's stored
    // value is only set at connect/materialize time and goes stale as turns
    // progress (e.g., turn 6 should be EARLY, not FRESH).
    let live_maturity =
        sidequest_game::world_materialization::CampaignMaturity::from_snapshot(&ctx.snapshot);
    if live_maturity != ctx.snapshot.campaign_maturity {
        tracing::info!(
            stored = ?ctx.snapshot.campaign_maturity,
            live = ?live_maturity,
            turn = ctx.snapshot.turn_manager.round(),
            "campaign_maturity.advanced — recalculated from turn count"
        );
    }

    // Process the action through GameService (FreePlay mode — immediate resolution)
    let context = TurnContext {
        state_summary: Some(state_summary),
        in_combat: ctx.in_combat(),
        in_chase: ctx.in_chase(),
        in_encounter: ctx.in_encounter(),
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
        // Story 23-4: lore filtering by graph distance
        world_graph: ctx.world_graph.clone(),
        // Story 15-18: progressive world materialization
        history_chapters: ctx.snapshot.world_history.clone(),
        campaign_maturity: live_maturity,
        // Multiplayer action attribution — narrator knows WHO is acting
        character_name: ctx.char_name.to_string(),
        // Genre-specific prompt templates from prompts.yaml
        genre_prompts: {
            let gs = ctx.genre_slug;
            sidequest_genre::GenreCode::new(gs)
                .ok()
                .and_then(|gc| {
                    ctx.state
                        .genre_cache()
                        .get_or_load(&gc, ctx.state.genre_loader())
                        .ok()
                })
                .map(|pack| pack.prompts.clone())
        },
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

    // Story 6-3: Update engagement counter based on intent classification.
    // Meaningful actions (Combat, Dialogue, Chase) reset the counter;
    // non-meaningful actions (Exploration, Examine, Meta) increment it.
    // The counter drives the trope tick multiplier via engagement_multiplier().
    {
        let is_meaningful = result
            .classified_intent
            .as_deref()
            .map(|i| matches!(i, "Combat" | "Dialogue" | "Chase"))
            .unwrap_or(false);
        if is_meaningful {
            ctx.snapshot.turns_since_meaningful = 0;
        } else {
            ctx.snapshot.turns_since_meaningful += 1;
        }
        tracing::info!(
            turns_since_meaningful = ctx.snapshot.turns_since_meaningful,
            is_meaningful = is_meaningful,
            "engagement.counter_updated"
        );
    }

    // Update preprocessed from inline agent output (approach A — no separate Haiku call).
    let _preprocessed =
        if let (Some(ref rw), Some(ref flags)) = (&result.action_rewrite, &result.action_flags) {
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
        .field("extraction_tier", &result.prompt_tier)
        .send();

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
            .send();
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
    // Try header extraction first (**Location**), fall back to game_patch JSON location field.
    let extracted_location = extract_location_header(narration_text).or_else(|| {
        if let Some(ref loc) = result.location {
            tracing::info!(
                location = %loc,
                "location.from_game_patch — header extraction missed, using JSON fallback"
            );
        }
        result.location.clone()
    });
    if let Some(location) = extracted_location {
        // Room-graph mode: resolve display name → room ID, then validate + apply.
        // Region mode (rooms empty): always valid — no room graph to check.
        let resolved_location = if !ctx.rooms.is_empty() {
            match sidequest_game::room_movement::resolve_room_id(&location, &ctx.rooms) {
                Some(id) => {
                    WatcherEventBuilder::new("room_graph", WatcherEventType::StateTransition)
                        .field("event", "room_graph.name_resolved")
                        .field("input", &location)
                        .field("resolved_id", id)
                        .send();
                    id.to_string()
                }
                None => {
                    WatcherEventBuilder::new("room_graph", WatcherEventType::ValidationWarning)
                        .field("event", "room_graph.name_unresolved")
                        .field("input", &location)
                        .send();
                    tracing::warn!(
                        name: "room_graph.name_unresolved",
                        input = %location,
                        "narrator used unknown room name — falling through to validation"
                    );
                    location.clone()
                }
            }
        } else {
            location.clone()
        };
        let location_valid = if !ctx.rooms.is_empty() {
            match sidequest_game::room_movement::apply_validated_move(
                ctx.snapshot,
                &resolved_location,
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
                        .send();

                    // Story 19-10: Deplete active light source on room transition
                    let remaining_before = ctx
                        .inventory
                        .items
                        .iter()
                        .find(|item| item.tags.iter().any(|t| t == "light"))
                        .and_then(|item| item.uses_remaining)
                        .unwrap_or(0);
                    if let Some(depleted_item) = ctx.inventory.deplete_light_on_transition() {
                        let item_name = depleted_item.name.as_str().to_owned();
                        tracing::info!(
                            name: "inventory.light_depleted",
                            item_name = %item_name,
                            remaining_before = remaining_before,
                        );
                        WatcherEventBuilder::new("inventory", WatcherEventType::StateTransition)
                            .field("event", "inventory.light_depleted")
                            .field("item_name", &item_name)
                            .field("remaining_before", &remaining_before.to_string())
                            .send();
                        messages.push(GameMessage::ItemDepleted {
                            payload: ItemDepletedPayload {
                                item_name,
                                remaining_before,
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
                        .send();
                    false
                }
            }
        } else {
            // Region mode — no room graph validation, but emit transition event
            // so the GM panel has from/to visibility (story 26-8).
            let from_location = ctx.current_location.clone();
            WatcherEventBuilder::new("location", WatcherEventType::StateTransition)
                .field("event", "region.transition")
                .field("from_location", &from_location)
                .field("to_location", &location)
                .field("mode", "region")
                .send();
            true
        };

        if location_valid {
            // In room_graph mode, use the resolved room ID as current_location
            // so build_room_graph_explored can match it. In region mode, use display name.
            let canonical_location = if !ctx.rooms.is_empty() {
                resolved_location.clone()
            } else {
                location.clone()
            };
            let is_new = !ctx.discovered_regions.iter().any(|r| r == &location);
            *ctx.current_location = canonical_location;
            if is_new {
                ctx.discovered_regions.push(location.clone());
                // Story 26-8: emit discovery event for GM panel visibility
                WatcherEventBuilder::new("location", WatcherEventType::StateTransition)
                    .field("event", "region.discovery")
                    .field("location", &location)
                    .field("turn_number", turn_number)
                    .field("total_discovered", ctx.discovered_regions.len())
                    .send();
                let summary = format!("Discovered {} on turn {}", location, turn_number);
                lore_sync::accumulate_and_persist_lore(
                    ctx,
                    &summary,
                    sidequest_game::lore::LoreCategory::Geography,
                    turn_number as u64,
                    std::collections::HashMap::new(),
                )
                .await;

                // POI image fast path: check for pre-rendered landscape image
                // on first location discovery. Images live in genre pack at
                // images/poi/{slug}.png, served via /genre/{slug}/...
                let location_slug = location
                    .to_lowercase()
                    .replace(' ', "_")
                    .replace('\'', "")
                    .replace('\u{2019}', "");
                let poi_image_path = ctx
                    .state
                    .genre_packs_path()
                    .join(ctx.genre_slug)
                    .join("images")
                    .join("poi")
                    .join(format!("{}.png", location_slug));
                if poi_image_path.exists() {
                    let served_url =
                        format!("/genre/{}/images/poi/{}.png", ctx.genre_slug, location_slug);
                    tracing::info!(
                        location = %location,
                        url = %served_url,
                        "poi_image.served — pre-rendered landscape on first discovery"
                    );
                    messages.push(GameMessage::Image {
                        payload: sidequest_protocol::ImagePayload {
                            url: served_url.clone(),
                            description: format!("{}", location),
                            handout: true,
                            render_id: None,
                            tier: Some("landscape".to_string()),
                            scene_type: Some("discovery".to_string()),
                            generation_ms: Some(0),
                        },
                        player_id: ctx.player_id.to_string(),
                    });
                    WatcherEventBuilder::new("poi_image", WatcherEventType::StateTransition)
                        .field("action", "poi_image_served")
                        .field("location", &location)
                        .field("slug", &location_slug)
                        .send();
                }
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
                .send();
            messages.push(GameMessage::ChapterMarker {
                payload: ChapterMarkerPayload {
                    title: Some(location.clone()),
                    location: Some(location.clone()),
                },
                player_id: ctx.player_id.to_string(),
            });
            let explored_locs: Vec<sidequest_protocol::ExploredLocation> = if !ctx.rooms.is_empty()
            {
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
                        tactical_grid: None,
                    })
                    .collect()
            };
            // Story 35-7: OTEL for tactical grid population in MAP_UPDATE
            let grids_populated = explored_locs
                .iter()
                .filter(|l| l.tactical_grid.is_some())
                .count();
            if grids_populated > 0 {
                WatcherEventBuilder::new("tactical_grid", WatcherEventType::StateTransition)
                    .field("event", "tactical_grid.map_update")
                    .field("rooms_with_grids", grids_populated)
                    .field("total_explored", explored_locs.len())
                    .send();
            }
            messages.push(GameMessage::MapUpdate {
                payload: MapUpdatePayload {
                    current_location: location,
                    region: ctx.current_location.clone(),
                    explored: explored_locs,
                    fog_bounds: None,
                    cartography: ctx.cartography_metadata.clone(),
                },
                player_id: ctx.player_id.to_string(),
            });
            WatcherEventBuilder::new("map", WatcherEventType::StateTransition)
                .field("event", "cartography_dispatch")
                .field("has_cartography", ctx.cartography_metadata.is_some())
                .field(
                    "navigation_mode",
                    ctx.cartography_metadata
                        .as_ref()
                        .map(|c| c.navigation_mode.as_str())
                        .unwrap_or("none"),
                )
                .send();
            ctx.turn_manager.advance_round();
            tracing::info!(
                new_round = ctx.turn_manager.round(),
                interaction = ctx.turn_manager.interaction(),
                "turn_manager.advance_round — location change"
            );
        }
    }

    let clean_narration = strip_fourth_wall(
        &strip_combat_brackets(&strip_fenced_blocks(&strip_location_header(narration_text)))
            .replace("</s>", "")
            .replace("<|endoftext|>", "")
            .replace("<|end|>", ""),
    );

    // Claiming handler stores narration so non-claimers can retrieve it
    // via get_resolution_narration() and skip the narrator call entirely.
    if let Some(ref outcome) = barrier_outcome {
        if outcome.claimed_resolution {
            outcome
                .barrier
                .store_resolution_narration(clean_narration.to_string());
            tracing::info!("barrier.claimer — stored narration for non-claimers");
        }
    }

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

    // NPC registry + OCEAN personality shifts + creature image detection
    let creature_images = npc_registry::update_npc_registry(ctx, &result, &clean_narration);
    for (creature_name, served_url) in &creature_images {
        let msg = GameMessage::Image {
            payload: sidequest_protocol::ImagePayload {
                url: served_url.clone(),
                description: format!("A {} appears", creature_name),
                handout: true,
                render_id: None,
                tier: Some("portrait".to_string()),
                scene_type: Some("exploration".to_string()),
                generation_ms: Some(0),
            },
            player_id: String::new(),
        };
        let _ = ctx.tx.send(msg).await;
    }

    // Story 35-2: Entity reference hot-path validation (informational OTEL only)
    {
        use sidequest_agents::entity_reference::{extract_potential_references, EntityRegistry};

        let registry = EntityRegistry::from_snapshot(&ctx.snapshot);
        let references = extract_potential_references(&clean_narration);
        for reference in &references {
            if !registry.matches(reference) {
                WatcherEventBuilder::new("entity_reference", WatcherEventType::ValidationWarning)
                    .field("unresolved_name", reference)
                    .field(
                        "narration_excerpt",
                        &clean_narration.chars().take(120).collect::<String>(),
                    )
                    .send();
            }
        }
    }

    // Story 35-3: Track questioned NPCs for scenario scoring.
    // When a scenario is active, any NPC from the scenario's role map that appears
    // in the narration is recorded as "questioned" for interrogation_breadth scoring.
    if let Some(ref mut scenario) = ctx.snapshot.scenario_state {
        if !scenario.is_resolved() {
            let npc_names: Vec<String> = scenario.npc_roles().keys().cloned().collect();
            for npc_name in &npc_names {
                if clean_narration.contains(npc_name.as_str()) {
                    scenario.record_questioned_npc(npc_name.clone());
                }
            }
        }
    }

    // Monster Manual: match narration against Manual NPCs, mark Active (ADR-059)
    {
        let mut activated = Vec::new();
        for npc in &ctx.monster_manual.npcs {
            if npc.state == sidequest_game::monster_manual::EntryState::Available
                && clean_narration.contains(&npc.name)
            {
                activated.push(npc.name.clone());
            }
        }
        for name in &activated {
            ctx.monster_manual.mark_active(name, ctx.current_location);
            tracing::info!(
                npc_name = %name,
                "monster_manual.npc_activated — narrator used pool NPC"
            );
            crate::WatcherEventBuilder::new(
                "monster_manual",
                crate::WatcherEventType::StateTransition,
            )
            .field("action", "npc_activated")
            .field("name", name)
            .send();
        }
    }

    // Story 15-14: Enrich registry with structured NPC data (age, appearance, pronouns)
    // from GameSnapshot.npcs — update_npc_registry only gets regex-extracted data.
    {
        let before: Vec<(String, bool, bool, bool)> = ctx
            .npc_registry
            .iter()
            .map(|e| {
                (
                    e.name.clone(),
                    e.pronouns.is_empty(),
                    e.age.is_empty(),
                    e.appearance.is_empty(),
                )
            })
            .collect();

        sidequest_game::enrich_registry_from_npcs(ctx.npc_registry, &ctx.snapshot.npcs);

        for (i, entry) in ctx.npc_registry.iter().enumerate() {
            if let Some((name, was_empty_pronouns, was_empty_age, was_empty_appearance)) =
                before.get(i)
            {
                let mut fields_added: u32 = 0;
                if *was_empty_pronouns && !entry.pronouns.is_empty() {
                    fields_added += 1;
                }
                if *was_empty_age && !entry.age.is_empty() {
                    fields_added += 1;
                }
                if *was_empty_appearance && !entry.appearance.is_empty() {
                    fields_added += 1;
                }
                if fields_added > 0 {
                    WatcherEventBuilder::new("npc_registry", WatcherEventType::StateTransition)
                        .field("event", "npc.registry_enriched")
                        .field("npc_name", name)
                        .field("fields_added", fields_added)
                        .send();
                }
            }
        }
    }

    // Story 15-19: Record conlang name knowledge for newly discovered NPCs
    // with names matching loaded name bank entries.
    {
        let turn = ctx.turn_manager.interaction();
        for npc in &result.npcs_present {
            if !npc.is_new {
                continue;
            }
            for bank in &ctx.name_banks {
                for generated_name in &bank.names {
                    if npc.name.contains(&generated_name.name) {
                        if let Ok(_frag_id) = sidequest_game::record_name_knowledge(
                            ctx.lore_store,
                            generated_name,
                            ctx.player_id,
                            turn,
                        ) {
                            WatcherEventBuilder::new("conlang", WatcherEventType::StateTransition)
                                .field("event", "name_recorded")
                                .field("name", &generated_name.name)
                                .field("language_id", &generated_name.language_id)
                                .field("gloss", &generated_name.gloss)
                                .send();

                            tracing::info!(
                                name = %generated_name.name,
                                language_id = %generated_name.language_id,
                                "conlang.name_recorded"
                            );
                        }
                    }
                }
            }
        }
    }

    // Story 15-19: Record conlang morphemes detected in narration text.
    // Scans each word of clean_narration against loaded morpheme glossaries.
    {
        let turn = ctx.turn_manager.interaction();
        let narration_lower = clean_narration.to_lowercase();
        for glossary in &ctx.morpheme_glossaries {
            for word in narration_lower.split_whitespace() {
                let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'');
                if trimmed.is_empty() {
                    continue;
                }
                if let Some(morpheme) = glossary.lookup(trimmed) {
                    if let Ok(_frag_id) = sidequest_game::record_language_knowledge(
                        ctx.lore_store,
                        morpheme,
                        ctx.player_id,
                        turn,
                    ) {
                        WatcherEventBuilder::new("conlang", WatcherEventType::StateTransition)
                            .field("event", "morpheme_learned")
                            .field("character_id", ctx.player_id)
                            .field("language_id", &morpheme.language_id)
                            .field("morpheme", &morpheme.morpheme)
                            .send();

                        tracing::info!(
                            morpheme = %morpheme.morpheme,
                            language_id = %morpheme.language_id,
                            "conlang.morpheme_learned"
                        );
                    }
                }
            }
        }
    }

    // State mutations must run before delta computation (delta depends on patched state).
    let mutation_result =
        state_mutations::apply_state_mutations(ctx, &result, &clean_narration, &effective_action)
            .await;
    let tier_events = mutation_result.tier_events;

    // Story 28-8: Encounter creation — the narrator signals a new encounter by emitting
    // `"confrontation": "combat"` (or any ConfrontationDef type) in the game_patch.
    // This creates a StructuredEncounter from the genre pack's ConfrontationDef and
    // populates actors from the player characters + NPCs present in the scene.
    if let Some(ref confrontation_type) = result.confrontation {
        if ctx.snapshot.encounter.is_none()
            || ctx
                .snapshot
                .encounter
                .as_ref()
                .map_or(false, |e| e.resolved)
        {
            if let Some(def) =
                crate::find_confrontation_def(&ctx.confrontation_defs, confrontation_type)
            {
                let mut encounter =
                    sidequest_game::encounter::StructuredEncounter::from_confrontation_def(def);

                // Populate actors: player characters + NPCs mentioned this turn
                for ch in &ctx.snapshot.characters {
                    encounter
                        .actors
                        .push(sidequest_game::encounter::EncounterActor {
                            name: ch.core.name.as_str().to_string(),
                            role: "player".to_string(),
                        });
                }
                // Add NPCs from this turn's narration (the narrator knows who's in the scene)
                for npc_mention in &result.npcs_present {
                    encounter
                        .actors
                        .push(sidequest_game::encounter::EncounterActor {
                            name: npc_mention.name.clone(),
                            role: "npc".to_string(),
                        });
                }

                WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                    .field("event", "encounter.created")
                    .field("encounter_type", confrontation_type)
                    .field("actor_count", encounter.actors.len())
                    .field("source", "narrator_confrontation")
                    .send();

                ctx.snapshot.encounter = Some(encounter);
            } else {
                tracing::warn!(
                    confrontation_type = %confrontation_type,
                    "encounter.creation_failed — no ConfrontationDef found for type"
                );
            }
        }
    }

    // Story 28-5: Beat selection dispatch — route narrator's beat_selection through
    // apply_beat() on the live StructuredEncounter. The beat's stat_check drives
    // resolution mechanics (attack → resolve_attack, escape → separation, others → metric_delta).
    //
    // Story 28-9: encounter_just_resolved is computed here (after beat dispatch),
    // not inside apply_state_mutations, because dispatch_beat_selection is what
    // actually sets encounter.resolved = true via apply_beat().
    let encounter_active_before_beat = ctx.in_encounter();

    // Story 28-6: Dispatch beat selections for ALL actors (player + NPCs) from
    // result.beat_selections. The narrator emits one entry per actor per round.
    if ctx.snapshot.encounter.is_some() {
        for bs in result.beat_selections.iter() {
            let actor = &bs.actor;
            let beat_id = &bs.beat_id;
            let target = bs.target.as_deref().unwrap_or("none");
            let is_player = actor.to_lowercase() == "player";

            // Dispatch the beat through resolution pipeline (player + NPC same path)
            beat::dispatch_beat_selection(ctx, beat_id);

            // OTEL: encounter.beat dispatched — GM panel lie detector
            let stat_check_result = ctx
                .snapshot
                .encounter
                .as_ref()
                .map(|e| format!("metric={}", e.metric.current))
                .unwrap_or_else(|| "no_encounter".to_string());

            WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                .field(
                    "event",
                    if is_player {
                        "encounter.player_beat"
                    } else {
                        "encounter.npc_beat"
                    },
                )
                .field("actor", actor)
                .field("beat_id", beat_id)
                .field("target", target)
                .field("stat_check", &stat_check_result)
                .send();
        }
    }

    // Legacy fallback: scene_intent carried beat_id before story 28-6 added
    // beat_selections. Only fires when no beat_selections are present.
    if result.beat_selections.is_empty() {
        if let Some(ref beat_id) = result.scene_intent {
            if ctx.snapshot.encounter.is_some() {
                beat::dispatch_beat_selection(ctx, beat_id);
            }
        }
    }

    let encounter_active_after_beat = ctx.in_encounter();
    let encounter_just_resolved = encounter_active_before_beat && !encounter_active_after_beat;
    let encounter_just_started = !encounter_active_before_beat && encounter_active_after_beat;

    // OTEL: encounter state transitions (story 28-9)
    if encounter_just_resolved {
        WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
            .field("event", "encounter.resolved")
            .field(
                "encounter_type",
                ctx.snapshot
                    .encounter
                    .as_ref()
                    .map_or("unknown", |e| &e.encounter_type),
            )
            .field("turn", turn_number)
            .send();
        // Clear resolved encounter so the overlay doesn't keep broadcasting
        ctx.snapshot.encounter = None;
    }
    if encounter_just_started {
        WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
            .field("event", "encounter.started")
            .field(
                "encounter_type",
                ctx.snapshot
                    .encounter
                    .as_ref()
                    .map_or("unknown", |e| &e.encounter_type),
            )
            .field("turn", turn_number)
            .send();
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

    // Send RENDER_QUEUED *before* narration so the UI has the placeholder
    // when it processes the NARRATION message.  Previously this ran in the
    // media section after narration, so the placeholder never existed when
    // buildSegments tried to match render_id → images fell to the bottom.
    render::process_render(ctx, &clean_narration, narration_text, &result).await;

    // Build response messages (narration, party status, inventory)
    response::build_response_messages(
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

    // === Deferred post-narration work ===
    // These operations write to fields consumed by the NEXT turn only, not the current one.
    // Moved after build_response_messages so the client sees narration immediately
    // instead of waiting ~15s for the Haiku continuity call + daemon embed round-trips.

    // Continuity validation — LLM-based (Haiku), runs via spawn_blocking.
    // Gate: only call when there's meaningful state to validate against.
    // The validator's value is catching dead NPC resurrection and inventory
    // contradictions. Without dead NPCs, the ~15-22s Haiku subprocess call
    // returns zero contradictions almost every time — pure waste.
    {
        let dead_npcs_exist = ctx.npc_registry.iter().any(|n| n.max_hp > 0 && n.hp <= 0);
        let in_combat = ctx.in_combat();
        if in_combat {
            tracing::info!("continuity.skipped — in_combat, creature_smith output is structured");
            WatcherEventBuilder::new("continuity", WatcherEventType::SubsystemExerciseSummary)
                .field("action", "skipped")
                .field("reason", "in_combat")
                .send();
        } else if !dead_npcs_exist {
            tracing::info!("continuity.skipped — no dead NPCs, contradiction risk near-zero");
            WatcherEventBuilder::new("continuity", WatcherEventType::SubsystemExerciseSummary)
                .field("action", "skipped")
                .field("reason", "no_dead_npcs")
                .send();
        } else {
            lore_sync::validate_continuity(ctx, &clean_narration).await;
        }
    }

    // Lore accumulation — wire accumulate_lore into post-narration dispatch (story 15-7, AC-1)
    // lore_established is Vec<String> from narrator output — category defaults to Event.
    // Structured lore_established (with per-entry categories) requires narrator prompt changes — follow-up.
    if let Some(ref lore_entries) = result.lore_established {
        for entry in lore_entries {
            if entry.trim().is_empty() {
                continue;
            }
            tracing::info!("lore.fragment_category=Event (lore_established is unstructured Vec<String> — follow-up required for per-entry categories)");
            lore_sync::accumulate_and_persist_lore(
                ctx,
                entry,
                sidequest_game::lore::LoreCategory::Event,
                turn_number as u64,
                std::collections::HashMap::new(),
            )
            .await;
        }
    }

    drop(_state_update_guard);

    // Record encounter resolution as a lore event when encounter resolves this turn.
    if encounter_just_resolved {
        let summary = format!(
            "Encounter at {} concluded on turn {}",
            ctx.current_location, turn_number
        );
        lore_sync::accumulate_and_persist_lore(
            ctx,
            &summary,
            sidequest_game::lore::LoreCategory::Event,
            turn_number as u64,
            std::collections::HashMap::new(),
        )
        .await;
    }

    let system_tick_span = tracing::info_span!(
        "turn.system_tick",
        tropes_fired = tracing::field::Empty,
        achievements_earned = tracing::field::Empty,
    );
    let _system_tick_guard = system_tick_span.enter();

    // process_combat_and_chase deleted in story 28-9 — beat system handles encounters.

    let (fired_beats, earned_achievements) = {
        // Initialize TropeState for each definition if empty.  Definitions
        // are loaded in dispatch_connect but TropeState instances were never
        // created from them — the tick ran on an empty vec every turn.
        if ctx.trope_states.is_empty() && !ctx.trope_defs.is_empty() {
            for def in ctx.trope_defs.iter() {
                let id = def.id.as_deref().unwrap_or(def.name.as_str());
                ctx.trope_states
                    .push(sidequest_game::trope::TropeState::new(id));
            }
            tracing::info!(
                count = ctx.trope_states.len(),
                "trope_states.initialized — created from definitions (were empty)"
            );
        }
        let _tropes_guard = tracing::info_span!(
            "turn.system_tick.tropes",
            active_count = ctx.trope_states.len(),
        )
        .entered();
        tropes::process_tropes(ctx, &clean_narration, &mut messages)
    };
    system_tick_span.record("tropes_fired", fired_beats.len() as u64);
    system_tick_span.record("achievements_earned", earned_achievements.len() as u64);

    // ADR-073: collect beat data for TurnRecord before fired_beats is consumed.
    let turn_beats_for_record: Vec<(String, f32)> = fired_beats
        .iter()
        .map(|b| (b.trope_name.clone(), b.beat.at as f32))
        .collect();

    // Collect beat summaries for lore persistence before fired_beats is consumed by troper.
    let beat_lore_entries: Vec<(String, String)> = fired_beats
        .iter()
        .filter(|b| !b.beat.event.is_empty())
        .map(|b| {
            let summary = format!("{}: {}", b.trope_name, b.beat.event);
            (summary, b.trope_id.clone())
        })
        .collect();

    // Format beat context for NEXT turn's narrator prompt injection.
    // Beats fire after narration, so they inform the next turn — same as Python's
    // _pending_escalation_beats pattern.
    if !fired_beats.is_empty() {
        let _beat_ctx_guard = tracing::info_span!(
            "turn.system_tick.beat_context",
            beats_count = fired_beats.len(),
        )
        .entered();

        let mut troper = sidequest_agents::agents::troper::TroperAgent::new();
        troper.set_fired_beats(fired_beats);
        troper.set_trope_definitions(ctx.trope_defs.to_vec());
        troper.set_trope_states(ctx.trope_states.clone());
        *ctx.pending_trope_context = troper.build_beats_context();
    }

    // Persist trope beat descriptions as lore entries (Option B: collected before troper consumed them).
    for (summary, trope_id) in beat_lore_entries {
        let mut meta = std::collections::HashMap::new();
        meta.insert("trope_id".to_string(), trope_id);
        lore_sync::accumulate_and_persist_lore(
            ctx,
            &summary,
            sidequest_game::lore::LoreCategory::Event,
            turn_number as u64,
            meta,
        )
        .await;
    }

    // Epic 16: Resource pool decay — apply per-turn decay and mint threshold lore
    {
        let crossed = ctx.snapshot.apply_pool_decay();
        if !crossed.is_empty() {
            sidequest_game::mint_threshold_lore(&crossed, ctx.lore_store, turn_number as u64);
            for threshold in &crossed {
                WatcherEventBuilder::new("resource_pool", WatcherEventType::StateTransition)
                    .field("event", "resource_pool.threshold_crossed")
                    .field("event_id", &threshold.event_id)
                    .field("narrator_hint", &threshold.narrator_hint)
                    .field("at", threshold.at)
                    .field("source", "decay")
                    .field("turn", turn_number)
                    .send();
                tracing::info!(
                    event_id = %threshold.event_id,
                    at = threshold.at,
                    "resource_pool.threshold_crossed_by_decay"
                );
            }
        }

        // Phase 5: pool decay mutates snapshot.resources directly.
        // No sync needed — persistence reads snapshot.resources.
    }

    drop(_system_tick_guard);

    let media_span = tracing::info_span!(
        "turn.media",
        render_enqueued = tracing::field::Empty,
        audio_cue_sent = tracing::field::Empty,
    );
    let _media_guard = media_span.enter();

    // NOTE: process_render moved before build_response_messages (see above)
    // so RENDER_QUEUED arrives at the UI before NARRATION text.

    let location_changed = *ctx.current_location != location_before_turn;
    audio::process_audio(
        ctx,
        &clean_narration,
        &mut messages,
        &result,
        location_changed,
        encounter_just_resolved,
    )
    .await;

    // Record this interaction in the turn manager
    ctx.turn_manager.record_interaction();
    tracing::info!(
        interaction = ctx.turn_manager.interaction(),
        round = ctx.turn_manager.round(),
        "turn_manager.record_interaction"
    );

    drop(_media_guard);

    // Sync scattered locals into the canonical snapshot, then persist (story 15-8)
    persistence::sync_locals_to_snapshot(ctx, narration_text);

    // Story 15-20: compute state delta and broadcast typed messages
    let game_delta = {
        let after_snapshot = sidequest_game::delta::snapshot(ctx.snapshot);
        let delta = sidequest_game::delta::compute_delta(&before_snapshot, &after_snapshot);

        // OTEL event: delta.computed (story 15-20 AC)
        let changed_count = [
            delta.characters_changed(),
            delta.npcs_changed(),
            delta.location_changed(),
            delta.quest_log_changed(),
            delta.atmosphere_changed(),
            delta.regions_changed(),
            delta.tropes_changed(),
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
            is_empty = delta.is_empty(),
            "delta.computed"
        );

        // Generate typed broadcast messages from the delta
        let broadcast_msgs = sidequest_game::broadcast_state_changes(&delta, ctx.snapshot);
        for msg in broadcast_msgs {
            let _ = ctx.tx.send(msg).await;
        }

        delta
    };

    persistence::persist_game_state(ctx, narration_text, &clean_narration).await;

    // GM Panel snapshot + timing telemetry
    telemetry::emit_telemetry(
        ctx,
        turn_number,
        &result,
        turn_start,
        preprocess_done,
        agent_done,
    );

    // ADR-073: Construct and send TurnRecord for training data capture + OTEL bridge.
    if let Some(watcher_tx) = ctx.state.watcher_tx() {
        use sidequest_agents::agents::intent_router::Intent;
        use sidequest_agents::turn_record::{try_send_record, TurnRecord};

        let classified =
            Intent::from_display_str(result.classified_intent.as_deref().unwrap_or("Exploration"));

        let patches_applied = patching::derive_patches_from_delta(&game_delta);
        let after_game_snapshot = ctx.snapshot.clone();

        let record = TurnRecord {
            turn_id: ctx.state.next_turn_id(),
            timestamp: chrono::Utc::now(),
            player_input: ctx.action.to_string(),
            classified_intent: classified,
            agent_name: result.agent_name.clone().unwrap_or_default(),
            narration: result.narration.clone(),
            patches_applied,
            snapshot_before: before_game_snapshot,
            snapshot_after: after_game_snapshot,
            delta: game_delta,
            beats_fired: turn_beats_for_record,
            token_count_in: result.token_count_in.unwrap_or(0),
            token_count_out: result.token_count_out.unwrap_or(0),
            agent_duration_ms: result.agent_duration_ms.unwrap_or(0),
            is_degraded: result.is_degraded,
            spans: vec![],
            prompt_text: result.prompt_text.clone(),
            raw_response_text: result.raw_response_text.clone(),
        };
        try_send_record(watcher_tx, record);
    }

    let char_class: String = ctx
        .character_json
        .as_ref()
        .and_then(|cj| cj.get("char_class"))
        .and_then(|c| c.as_str())
        .unwrap_or("Adventurer")
        .to_string();

    // Perception filter population — map player's current status effects to
    // perceptual effects so multiplayer narration rewriting activates (story 8-6).
    if let Some(character) = ctx.snapshot.characters.first() {
        let perceptual_effects =
            barrier::map_statuses_to_perceptual_effects(&character.core.statuses);
        if !perceptual_effects.is_empty() {
            let holder = ctx.shared_session_holder.lock().await;
            if let Some(ref ss_arc) = *holder {
                let mut ss = ss_arc.lock().await;
                let char_name = character.core.name.as_str().to_string();
                ss.perception_filters.insert(
                    ctx.player_id.to_string(),
                    sidequest_game::perception::PerceptionFilter::new(
                        char_name,
                        perceptual_effects.clone(),
                    ),
                );
                WatcherEventBuilder::new("perception", WatcherEventType::StateTransition)
                    .field("event", "perception_filter_set")
                    .field("player_id", ctx.player_id)
                    .field(
                        "effects",
                        sidequest_game::perception::PerceptionRewriter::describe_effects(
                            &perceptual_effects,
                        )
                        .as_str(),
                    )
                    .send();
            }
        } else {
            // Clear any stale filter if no perceptual effects remain
            let holder = ctx.shared_session_holder.lock().await;
            if let Some(ref ss_arc) = *holder {
                let mut ss = ss_arc.lock().await;
                if ss.perception_filters.remove(ctx.player_id).is_some() {
                    WatcherEventBuilder::new("perception", WatcherEventType::StateTransition)
                        .field("event", "perception_filter_cleared")
                        .field("player_id", ctx.player_id)
                        .send();
                }
            }
        }
    }

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

    /// Story 15-26: Verify process_between_turns is called in the dispatch pipeline
    /// so NPC autonomous actions are mechanically selected, not LLM-improvised.
    #[test]
    fn dispatch_pipeline_calls_process_between_turns() {
        let source = include_str!("mod.rs");
        let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(
            production_code.contains("process_between_turns("),
            "dispatch must call scenario_state.process_between_turns() to select \
             NPC actions mechanically — story 15-26 (Pattern 5 fix)"
        );
    }

    /// Story 15-26 / 7-9: Verify OTEL events are emitted for NPC action selection
    /// under the unified "scenario" namespace so the GM panel can filter by subsystem.
    #[test]
    fn dispatch_pipeline_emits_npc_action_otel() {
        let source = include_str!("mod.rs");
        let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(
            production_code.contains("scenario.npc_action"),
            "dispatch must emit scenario.npc_action OTEL event for each NPC \
             autonomous action — story 7-9 (unified scenario namespace)"
        );
    }

    /// Story 15-26: Verify NPC actions are injected into narrator context
    /// so the narrator writes around mechanical decisions, not inventing them.
    #[test]
    fn dispatch_pipeline_injects_npc_actions_into_prompt() {
        let source = include_str!("mod.rs");
        let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(
            production_code.contains("NPC AUTONOMOUS ACTIONS THIS TURN"),
            "dispatch must inject NPC action descriptions into state_summary \
             for narrator context — story 15-26"
        );
    }
}

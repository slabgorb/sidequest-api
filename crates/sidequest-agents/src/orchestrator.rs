//! Orchestrator — state machine that sequences agents and manages game state.
//!
//! Port lesson #1: Server talks to GameService trait, never game internals.
//! ADR-010: Intent-based agent routing via LLM classification.

use std::collections::HashMap;

use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::agent::Agent;
use crate::lore_filter::LoreFilter;
use crate::tools::assemble_turn::assemble_turn;
// ADR-059: parse_tool_results removed — Monster Manual replaces sidecar mechanism
// ADR-067: CreatureSmith, Dialectician, Ensemble absorbed into unified narrator
use crate::agents::intent_router::{Intent, IntentRoute, IntentRouter};
use crate::agents::narrator::NarratorAgent;
use crate::agents::troper::TroperAgent;
use crate::agents::world_builder::WorldBuilderAgent;
use crate::client::ClaudeClient;
use crate::context_builder::{ContextBuilder, ZoneBreakdown};
use crate::prompt_framework::{parse_soul_md, AttentionZone, PromptSection, SectionCategory};
use crate::turn_record::{TurnIdCounter, TurnRecord};
use sidequest_game::merchant::format_merchant_context;
use sidequest_game::npc::{Npc, NpcRegistryEntry};
use sidequest_game::tension_tracker::{DeliveryMode, DramaThresholds, TensionTracker};

/// Result of processing a player action through the orchestrator.
#[derive(Debug, Clone)]
pub struct ActionResult {
    /// The narrative text produced by the agent.
    pub narration: String,
    /// Beat selections extracted from narrator output (story 28-6).
    /// Each entry maps an actor to a beat_id from the active ConfrontationDef.
    /// Dispatched via apply_beat() in the server dispatch pipeline (story 28-5).
    pub beat_selections: Vec<BeatSelection>,
    /// Whether this is a degraded response (e.g., from agent timeout).
    pub is_degraded: bool,
    /// Which intent was classified (for OTEL telemetry).
    pub classified_intent: Option<String>,
    /// Which agent handled the action (for OTEL telemetry).
    pub agent_name: Option<String>,
    /// Structured footnotes extracted from narrator output (story 9-11).
    /// New discoveries feed into the knowledge pipeline via footnotes_to_discovered_facts.
    pub footnotes: Vec<sidequest_protocol::Footnote>,
    /// Items gained by the player this turn (extracted from narrator JSON block).
    pub items_gained: Vec<sidequest_protocol::ItemGained>,
    /// NPCs present in the narrator's response (extracted from narrator JSON block).
    pub npcs_present: Vec<NpcMention>,
    /// Quest log updates extracted from narrator JSON block.
    pub quest_updates: HashMap<String, String>,
    /// Wall-clock duration of the agent LLM call in milliseconds (for GM Dashboard).
    pub agent_duration_ms: Option<u64>,
    /// Input tokens consumed by the agent LLM call (for GM Dashboard).
    pub token_count_in: Option<usize>,
    /// Output tokens produced by the agent LLM call (for GM Dashboard).
    pub token_count_out: Option<usize>,
    /// Visual scene description for image generation (from narrator JSON block).
    pub visual_scene: Option<VisualScene>,
    /// Scene mood tag for atmosphere/music selection (from narrator JSON block).
    pub scene_mood: Option<String>,
    /// OCEAN personality events for NPCs present this turn (from narrator JSON block).
    pub personality_events: Vec<PersonalityEvent>,
    /// High-level narrative intent of this scene (from narrator JSON block).
    pub scene_intent: Option<String>,
    /// Resource deltas extracted from narrator JSON block (story 16-1).
    /// Keyed by resource name (e.g., "luck", "humanity"), values are signed deltas.
    pub resource_deltas: HashMap<String, f64>,
    /// Zone breakdown of the assembled prompt (story 18-6).
    /// Used by the Prompt Inspector dashboard tab.
    pub zone_breakdown: Option<ZoneBreakdown>,
    /// Lore fragments established during this turn (story 15-7).
    /// Extracted from narrator structured JSON block, fed to `accumulate_lore()` in dispatch.
    pub lore_established: Option<Vec<String>>,
    /// Merchant transactions extracted from narrator JSON block (story 15-16).
    /// Converted to MerchantTransactionRequests and applied via apply_merchant_transactions().
    pub merchant_transactions: Vec<MerchantTransactionExtracted>,
    /// SFX trigger IDs chosen by the narrator based on what happened in the scene.
    /// Passed through to AudioCuePayload.sfx_triggers for client playback.
    pub sfx_triggers: Vec<String>,
    /// Inline preprocessor: action rewrite (eliminates separate Haiku subprocess).
    pub action_rewrite: Option<ActionRewrite>,
    /// Inline preprocessor: relevance flags.
    pub action_flags: Option<ActionFlags>,
    /// Narrator prompt tier used for this turn (ADR-066): "full" or "delta".
    pub prompt_tier: String,
    /// Confrontation type to initiate this turn (story 28-8).
    /// When the narrator emits `"confrontation": "combat"`, the server creates
    /// a StructuredEncounter via from_confrontation_def(). None = no new encounter.
    pub confrontation: Option<String>,
    /// Location name from game_patch JSON (fallback when header extraction returns None).
    pub location: Option<String>,
    /// Full assembled prompt text for training data capture (ADR-073).
    pub prompt_text: Option<String>,
    /// Raw LLM response text before extraction (ADR-073).
    pub raw_response_text: Option<String>,
}

/// A single beat selection from the narrator's output (story 28-6).
/// The narrator picks beats from the ConfrontationDef's available beat list.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct BeatSelection {
    /// Who performs the action (e.g., "Player", "Goblin").
    pub actor: String,
    /// Which beat from the ConfrontationDef (e.g., "attack", "bluff", "escape").
    pub beat_id: String,
    /// Optional target of the action (e.g., "Goblin", "Door").
    #[serde(default)]
    pub target: Option<String>,
}

/// Narrator prompt tier (ADR-066). Controls how much context is included.
/// Modeled after Pennyfarthing's prime tiered context system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NarratorPromptTier {
    /// Full context — first turn of a new session. Everything included.
    Full,
    /// Delta only — subsequent turns on a resumed session. Static context
    /// (agent identity, SOUL, SFX library, verbosity rules) is already in
    /// the conversation history. Only dynamic state + action sent.
    Delta,
}

/// Result of building a narrator prompt without invoking the LLM.
///
/// Extracted from `process_action()` so prompt content can be tested
/// independently of the Claude CLI subprocess (story 15-27).
#[derive(Debug, Clone)]
pub struct NarratorPromptResult {
    /// The fully composed prompt text, ordered by attention zone.
    pub prompt_text: String,
    /// Zone breakdown for the Prompt Inspector dashboard tab.
    pub zone_breakdown: ZoneBreakdown,
    /// Names of script tools whose instruction sections were injected into the prompt.
    /// Empty when genre is None or no tools are registered.
    pub script_tools_injected: Vec<String>,
    /// The `--allowedTools` specs for the Claude CLI subprocess.
    pub allowed_tools: Vec<String>,
    /// The intent classification result, so callers don't need to re-classify.
    pub intent_route: IntentRoute,
    /// Environment variables to set on the Claude CLI subprocess (story 23-11).
    /// Contains `SIDEQUEST_GENRE` and `SIDEQUEST_CONTENT_PATH` when script tools are injected.
    pub env_vars: HashMap<String, String>,
}

/// Configuration for a script tool binary (ADR-056).
///
/// Each tool is a Rust binary that generates game objects from genre pack data.
/// The narrator subprocess gets `--allowedTools Bash(...)` to invoke these tools
/// autonomously during narration.
#[derive(Debug, Clone)]
pub struct ScriptToolConfig {
    /// Absolute path to the tool binary.
    pub binary_path: String,
    /// Absolute path to the `genre_packs/` directory.
    pub genre_packs_path: String,
}

/// Facade trait for the game engine. Server depends on this, never on internals.
///
/// ADR-005: Graceful degradation — timeout produces a degraded ActionResult, not an error.
pub trait GameService: Send + Sync {
    /// Get a snapshot of the current game state.
    fn get_snapshot(&self) -> serde_json::Value;

    /// Process a player action and return narration + state changes.
    fn process_action(&self, action: &str, context: &TurnContext) -> ActionResult;

    /// Reset the narrator session, forcing the next prompt to use Full tier.
    /// Call when a player connects to a different game (story 30-2).
    fn reset_narrator_session_for_connect(&self);
}

/// The orchestrator state machine. Implements GameService.
///
/// Routes player input → intent classification → agent dispatch → patch application → delta.
pub struct Orchestrator {
    /// Sender end of the watcher channel for TurnRecord delivery.
    pub watcher_tx: mpsc::Sender<TurnRecord>,
    /// Monotonically increasing turn ID counter.
    pub turn_id_counter: TurnIdCounter,
    /// Claude CLI client for LLM invocations.
    client: ClaudeClient,
    /// State-based intent inference (ADR-067: no LLM classification).
    intent_router: IntentRouter,
    /// Unified narrator agent (ADR-067: absorbs combat, chase, dialogue).
    narrator: NarratorAgent,
    /// Pacing engine — tracks drama weight across combat turns (Story 5-7).
    tension_tracker: TensionTracker,
    /// Genre-tunable pacing breakpoints (Story 5-7).
    drama_thresholds: DramaThresholds,
    /// Trope beat injection agent (ADR-018).
    ///
    /// Currently unwired — see `sidequest-agents/CLAUDE.md` → "NEEDS FULL
    /// IMPLEMENTATION" section. The TropeEngine in `sidequest-game/trope.rs`
    /// handles passive ticking, but LLM-driven beat injection is not yet
    /// orchestrated from here.
    #[allow(dead_code)]
    troper: TroperAgent,
    /// SOUL.md principles — filtered per agent via `<agents>` tags and injected in the Early zone.
    soul_data: Option<crate::prompt_framework::SoulData>,
    /// Script tool configurations (ADR-056). Keyed by tool name.
    script_tools: HashMap<String, ScriptToolConfig>,
    /// Persistent narrator session ID (ADR-066). Set on first narrator call,
    /// reused via `--resume` on subsequent turns. Mutex because `process_action`
    /// takes `&self` (GameService trait constraint).
    narrator_session_id: std::sync::Mutex<Option<String>>,
    /// Genre slug that established the current narrator session (story 30-2).
    /// Used to detect genre switches — if the incoming TurnContext has a different
    /// genre, the session is stale and must be reset to Full tier.
    session_genre: std::sync::Mutex<Option<String>>,
}

impl Orchestrator {
    /// Create a new orchestrator with a watcher channel sender.
    ///
    /// Automatically loads SOUL.md from the current working directory if present.
    /// SOUL principles are injected into every agent prompt in the Early attention zone.
    pub fn new(watcher_tx: mpsc::Sender<TurnRecord>) -> Self {
        Self::new_with_otel(watcher_tx, None)
    }

    /// Create a new orchestrator with optional OTEL endpoint for Claude subprocess telemetry.
    pub fn new_with_otel(
        watcher_tx: mpsc::Sender<TurnRecord>,
        otel_endpoint: Option<String>,
    ) -> Self {
        let client = if let Some(endpoint) = otel_endpoint {
            ClaudeClient::builder().otel_endpoint(endpoint).build()
        } else {
            ClaudeClient::new()
        };
        let soul_path = std::path::Path::new("SOUL.md");
        let soul_data = parse_soul_md(soul_path);
        let soul_data = if soul_data.is_empty() {
            info!("SOUL.md not found or empty — prompts will lack guiding principles");
            None
        } else {
            info!(
                principles = soul_data.len(),
                "SOUL.md loaded — {} principles available, filtered per agent via <agents> tags",
                soul_data.len()
            );
            Some(soul_data)
        };
        Self {
            watcher_tx,
            turn_id_counter: TurnIdCounter::new(),
            intent_router: IntentRouter::new(client.clone()),
            client,
            narrator: NarratorAgent::new(),
            tension_tracker: TensionTracker::new(),
            drama_thresholds: DramaThresholds::default(),
            troper: TroperAgent::new(),
            soul_data,
            script_tools: HashMap::new(),
            narrator_session_id: std::sync::Mutex::new(None),
            session_genre: std::sync::Mutex::new(None),
        }
    }

    /// Create an orchestrator for testing — no watcher channel needed.
    /// Uses a bounded channel internally but the receiver is dropped immediately.
    pub fn new_for_test() -> Self {
        let (tx, _rx) = mpsc::channel(1);
        Self::new(tx)
    }

    // === Narrator Session Lifecycle (story 30-2) ===

    /// Reset the narrator session, forcing the next prompt to use Full tier.
    /// Call this when switching games, loading a different save, or when the
    /// narrator has lost context and needs a full grounding prompt.
    pub fn reset_narrator_session(&self) {
        let _span = tracing::info_span!(
            "orchestrator.narrator_session_reset",
            reason = "session_lifecycle",
        )
        .entered();
        *self.narrator_session_id.lock().unwrap() = None;
        *self.session_genre.lock().unwrap() = None;
    }

    /// Set the narrator session ID (for testing and server dispatch).
    pub fn set_narrator_session_id(&self, id: String) {
        *self.narrator_session_id.lock().unwrap() = Some(id);
    }

    /// Check whether a narrator session is currently active.
    pub fn has_active_narrator_session(&self) -> bool {
        self.narrator_session_id.lock().unwrap().is_some()
    }

    /// Record which genre established the current narrator session.
    pub fn set_session_genre(&self, genre: String) {
        *self.session_genre.lock().unwrap() = Some(genre);
    }

    /// Select the prompt tier based on session state and genre match.
    /// Returns Full if no session exists or if the genre has changed
    /// since the session was established (stale session detection).
    pub fn select_prompt_tier(&self, context: &TurnContext) -> NarratorPromptTier {
        let current_session = self.narrator_session_id.lock().unwrap().is_some();
        if !current_session {
            return NarratorPromptTier::Full;
        }

        // Genre switch detection: if the incoming genre differs from the
        // genre that established the session, the cached session is stale.
        // Must drop the lock before calling reset_narrator_session() to avoid deadlock.
        let genre_mismatch = if let Some(ref incoming_genre) = context.genre {
            let session_genre = self.session_genre.lock().unwrap();
            if let Some(ref stored_genre) = *session_genre {
                stored_genre != incoming_genre
            } else {
                false
            }
        } else {
            false
        };

        if genre_mismatch {
            let incoming = context.genre.as_deref().unwrap_or("unknown");
            tracing::warn!(
                incoming_genre = %incoming,
                "Genre switch detected — clearing stale session and forcing Full tier"
            );
            self.reset_narrator_session();
            return NarratorPromptTier::Full;
        }

        NarratorPromptTier::Delta
    }

    /// Replace the SOUL data for testing (story 23-10).
    pub fn set_soul_data(&mut self, soul: crate::prompt_framework::SoulData) {
        self.soul_data = Some(soul);
    }

    /// Register a script tool binary (ADR-056).
    pub fn register_script_tool(&mut self, name: &str, config: ScriptToolConfig) {
        info!(
            tool = %name,
            binary = %config.binary_path,
            "Script tool registered"
        );
        self.script_tools.insert(name.to_string(), config);
    }

    /// Build the `--allowedTools` spec for the narrator subprocess.
    ///
    /// Returns tool spec strings that let the narrator invoke registered script tools
    /// via `Bash(...)`. Empty if no tools are configured.
    pub fn narrator_allowed_tools(&self) -> Vec<String> {
        // ADR-059: No tools needed. Monster Manual injects data via game_state.
        // Claude narrates, engine crunches. No --allowedTools passed to claude -p.
        Vec::new()
    }

    /// Access the tension tracker (pacing engine).
    pub fn tension(&self) -> &TensionTracker {
        &self.tension_tracker
    }

    /// Access the drama thresholds (genre-tunable pacing breakpoints).
    pub fn drama_thresholds(&self) -> &DramaThresholds {
        &self.drama_thresholds
    }

    /// Build the narrator prompt and tool configuration without invoking the LLM.
    ///
    /// Extracted from `process_action()` (story 15-27) so prompt content — including
    /// script tool sections — can be tested independently. Runs intent classification
    /// internally to determine which agent's system prompt to use.
    ///
    /// Returns the composed prompt text, zone breakdown, injected script tool names,
    /// and the `--allowedTools` specs for the Claude CLI.
    pub fn build_narrator_prompt(
        &self,
        action: &str,
        context: &TurnContext,
    ) -> NarratorPromptResult {
        self.build_narrator_prompt_tiered(action, context, NarratorPromptTier::Full)
    }

    /// Build the narrator prompt with tiered context (ADR-066).
    ///
    /// `Full` includes everything (first turn). `Delta` skips static sections
    /// that are already in the persistent session's conversation history.
    pub fn build_narrator_prompt_tiered(
        &self,
        action: &str,
        context: &TurnContext,
        tier: NarratorPromptTier,
    ) -> NarratorPromptResult {
        let route = self.intent_router.classify(action, context);

        let mut builder = ContextBuilder::new();
        let script_tools_injected: Vec<String> = Vec::new();
        let is_full = tier == NarratorPromptTier::Full;

        // === STATIC SECTIONS (Full tier only — already in session history on Delta) ===

        if is_full {
            // ADR-067: Always narrator identity (unified agent)
            self.narrator.build_context(&mut builder);

            // Always inject dialogue rules — they're short and NPCs can appear anytime
            self.narrator.build_dialogue_context(&mut builder);

            // SOUL principles (Early zone — high attention, after identity, before state)
            if let Some(ref soul) = self.soul_data {
                let filtered = soul.as_prompt_text_for(route.agent_name());
                if !filtered.is_empty() {
                    builder.add_section(PromptSection::new(
                        "soul_principles",
                        filtered,
                        AttentionZone::Early,
                        SectionCategory::Soul,
                    ));
                }
            }
        }

        // === OUTPUT FORMAT (every tier — narrator must always know game_patch schema) ===
        // The confrontation field and other structured output formats must be present
        // on Delta tier too, not just Full. Without this, the narrator on resumed
        // sessions doesn't know how to emit confrontation to start encounters.
        self.narrator.build_output_format(&mut builder);

        // === GENRE IDENTITY (every tier — narrator MUST always know the genre) ===
        // Without this, the LLM has no genre context and may break character by asking
        // the player what genre they're in. Injected in Primacy zone for maximum attention.
        // Fix: playtest-2026-04-05 — narrator broke fourth wall asking "What genre is Ashgate Square in?"
        if let Some(ref genre_slug) = context.genre {
            let genre_display = genre_slug.replace('_', " ");
            let _genre_span = tracing::info_span!(
                "orchestrator.genre_identity_injection",
                genre = %genre_slug,
                tier = ?tier,
            )
            .entered();
            builder.add_section(PromptSection::new(
                "genre_identity",
                format!(
                    "<genre>\nYou are narrating a {} game. This is the genre — \
                     use its tone, vocabulary, tropes, and conventions in every response. \
                     Never ask the player what genre, setting, or system they are playing. \
                     You already know.\n</genre>",
                    genre_display
                ),
                AttentionZone::Primacy,
                SectionCategory::Identity,
            ));
        }

        // === GENRE PROMPT TEMPLATES (from prompts.yaml) ===
        if let Some(ref gp) = context.genre_prompts {
            // Narrator voice — every tier (story 30-2: narrator loses genre voice on Delta)
            if !gp.narrator.is_empty() {
                builder.add_section(PromptSection::new(
                    "genre_narrator_voice",
                    format!("<genre-voice>\n{}\n</genre-voice>", gp.narrator),
                    AttentionZone::Primacy,
                    SectionCategory::Identity,
                ));
            }

            // NPC behavior — every tier (story 30-2: NPCs lose genre personality on Delta)
            if !gp.npc.is_empty() {
                builder.add_section(PromptSection::new(
                    "genre_npc_voice",
                    format!("<genre-npc>\n{}\n</genre-npc>", gp.npc),
                    AttentionZone::Early,
                    SectionCategory::Genre,
                ));
            }

            // World state tracking — every tier (story 30-2: narrator stops tracking state on Delta)
            if !gp.world_state.is_empty() {
                builder.add_section(PromptSection::new(
                    "genre_world_state",
                    format!(
                        "<genre-world-state>\n{}\n</genre-world-state>",
                        gp.world_state
                    ),
                    AttentionZone::Early,
                    SectionCategory::Genre,
                ));
            }

            // Combat — every tier (combat can start mid-session)
            if context.in_combat && !gp.combat.is_empty() {
                builder.add_section(PromptSection::new(
                    "genre_combat_voice",
                    format!("<genre-combat>\n{}\n</genre-combat>", gp.combat),
                    AttentionZone::Early,
                    SectionCategory::Genre,
                ));
            }

            // Chase — every tier (chase can start mid-session)
            if context.in_chase {
                if let Some(ref chase) = gp.chase {
                    if !chase.is_empty() {
                        builder.add_section(PromptSection::new(
                            "genre_chase_voice",
                            format!("<genre-chase>\n{}\n</genre-chase>", chase),
                            AttentionZone::Early,
                            SectionCategory::Genre,
                        ));
                    }
                }
            }

            // Extraction — every tier (extraction can trigger mid-session)
            if let Some(ref extraction) = gp.extraction {
                if !extraction.is_empty() {
                    builder.add_section(PromptSection::new(
                        "genre_extraction",
                        format!("<genre-extraction>\n{}\n</genre-extraction>", extraction),
                        AttentionZone::Valley,
                        SectionCategory::Genre,
                    ));
                }
            }

            // Keeper monologue — Full tier only
            if is_full {
                if let Some(ref keeper) = gp.keeper_monologue {
                    if !keeper.is_empty() {
                        builder.add_section(PromptSection::new(
                            "genre_keeper_monologue",
                            format!("<genre-keeper>\n{}\n</genre-keeper>", keeper),
                            AttentionZone::Valley,
                            SectionCategory::Genre,
                        ));
                    }
                }
            }

            // Town — Full tier only
            if is_full {
                if let Some(ref town) = gp.town {
                    if !town.is_empty() {
                        builder.add_section(PromptSection::new(
                            "genre_town",
                            format!("<genre-town>\n{}\n</genre-town>", town),
                            AttentionZone::Valley,
                            SectionCategory::Genre,
                        ));
                    }
                }
            }

            // Chargen — Full tier only
            if is_full {
                if let Some(ref chargen) = gp.chargen {
                    if !chargen.is_empty() {
                        builder.add_section(PromptSection::new(
                            "genre_chargen",
                            format!("<genre-chargen>\n{}\n</genre-chargen>", chargen),
                            AttentionZone::Valley,
                            SectionCategory::Genre,
                        ));
                    }
                }
            }

            // Transition hints — Full tier only (stable vocabulary)
            if is_full && !gp.transition_hints.is_empty() {
                let hints: Vec<String> = gp
                    .transition_hints
                    .iter()
                    .map(|(k, v)| format!("  {}: \"{}\"", k, v))
                    .collect();
                builder.add_section(PromptSection::new(
                    "genre_transition_hints",
                    format!("transition_hints:\n{}", hints.join("\n")),
                    AttentionZone::Late,
                    SectionCategory::Format,
                ));
            }
        }

        // === STATE-DEPENDENT SECTIONS (every tier — encounters can start mid-session) ===
        // Story 28-8: inject encounter rules for ANY active encounter, not just combat/chase.
        // Standoffs, negotiations, and other ConfrontationDef types also need encounter context.
        if context.in_combat || context.in_chase || context.in_encounter {
            self.narrator.build_encounter_context(&mut builder);
        }

        // Trope beat directives (Early zone)
        if let Some(ref beats) = context.pending_trope_context {
            let _trope_span =
                tracing::info_span!("orchestrator.trope_beat_injection", beats_injected = 1u64,)
                    .entered();
            builder.add_section(PromptSection::new(
                "trope_beat_directives",
                beats.clone(),
                AttentionZone::Early,
                SectionCategory::State,
            ));
        }

        // Tool workflow (Primacy zone — mandatory procedure before narration)
        // ADR-059: Tool infrastructure removed. Monster Manual injects NPC/encounter
        // data into game_state via dispatch. No tool sections, no env vars, no PATH.
        let env_vars: HashMap<String, String> = HashMap::new();

        // Game state section (Valley zone)
        if let Some(state) = &context.state_summary {
            builder.add_section(PromptSection::new(
                "game_state",
                format!("<game_state>\n{}\n</game_state>", state),
                AttentionZone::Valley,
                SectionCategory::State,
            ));
        }

        // Progressive world materialization (story 15-18)
        // Only inject narrator-facing world context (maturity tag + materialized
        // history chapters). Does NOT include the world builder's system prompt —
        // that would leak agent instructions into the narrator's context.
        if !context.history_chapters.is_empty() {
            let world_agent = WorldBuilderAgent::new()
                .with_maturity(context.campaign_maturity.clone())
                .with_chapters(context.history_chapters.clone());
            world_agent.inject_world_context(&mut builder);
        }

        // Lore filtering by graph distance (Valley zone — story 23-4)
        // When a hierarchical world graph is available, inject lore sections
        // at detail levels determined by graph distance from current node.
        if let Some(ref world_graph) = context.world_graph {
            let filter = LoreFilter::new(world_graph);
            let selections = filter.select_lore(
                &context.current_location,
                route.intent(),
                &context.npc_registry,
                &[], // Arc proximity: future enrichment via TropeState
            );

            let otel_summary = filter.format_otel_summary(&selections);
            let _lore_span = tracing::info_span!(
                "orchestrator.lore_filter",
                current_node = %context.current_location,
                intent = ?route.intent(),
                total_selections = selections.len() as u64,
                summary = %otel_summary,
            )
            .entered();

            let lore_content = LoreFilter::format_prompt_section(&selections);

            if !lore_content.is_empty() {
                builder.add_section(PromptSection::new(
                    "world_lore",
                    format!("<world-lore>\n{}</world-lore>", lore_content),
                    AttentionZone::Valley,
                    SectionCategory::Context,
                ));
            }
        }

        // Merchant context injection (Valley zone — story 15-16)
        inject_merchant_context(
            &mut builder,
            &context.npc_registry,
            &context.npcs,
            route.intent(),
            &context.current_location,
        );

        // Active trope summary (Valley zone)
        if let Some(ref trope_summary) = context.active_trope_summary {
            builder.add_section(PromptSection::new(
                "active_tropes",
                trope_summary.clone(),
                AttentionZone::Valley,
                SectionCategory::State,
            ));
        }

        // SFX library (Valley zone) — static, only on Full tier
        if is_full && !context.available_sfx.is_empty() {
            let sfx_list = context.available_sfx.join(", ");
            builder.add_section(PromptSection::new(
                "sfx_library",
                format!(
                    "[AVAILABLE SFX]\n\
                     When your narration describes a sound-producing action, include matching \
                     SFX IDs in sfx_triggers. Pick based on what HAPPENED, not what was mentioned.\n\
                     Available: {}",
                    sfx_list
                ),
                AttentionZone::Valley,
                SectionCategory::State,
            ));
        }

        // Backstory capture directive — static format, only on Full tier
        if is_full && route.intent() == Intent::Backstory {
            builder.add_section(PromptSection::new(
                "backstory_capture",
                "## Backstory Capture Mode\n\
                 The player is describing their character's history, personality, appearance, \
                 possessions, or memories. This is character-building, not plot advancement.\n\n\
                 IMPORTANT: In your JSON footnotes block, extract each personal detail as a \
                 separate footnote with `is_new: true`. Examples of what to capture:\n\
                 - Physical description or appearance details\n\
                 - Personal history or backstory events\n\
                 - Relationships to people or places from their past\n\
                 - Emotional traits, habits, or personality quirks\n\
                 - Meaningful possessions, keepsakes, or mementos\n\
                 - Skills, training, or formative experiences\n\n\
                 Each footnote summary should be a concise, third-person statement about the \
                 character (e.g., \"Served in the Union army\" not \"The player mentioned serving\"). \
                 These facts will be stored permanently and recalled when relevant."
                    .to_string(),
                AttentionZone::Late,
                SectionCategory::Format,
            ));
        }

        // Narrator verbosity (Recency zone) — injected on EVERY turn, not just Full.
        // Length limits must be in the highest-attention zone to prevent verbose responses.
        {
            use sidequest_protocol::NarratorVerbosity;
            let content = match context.narrator_verbosity {
                NarratorVerbosity::Concise => {
                    "<length-limit>\n\
                     Target: 2-4 sentences, around 400 characters of prose. \
                     Action and consequence first. Brief scene-setting only on arrivals. \
                     Keep it punchy — this mode prioritizes pace over atmosphere.\n\
                     </length-limit>"
                }
                NarratorVerbosity::Standard => {
                    "<length-limit>\n\
                     Target: 2-3 short paragraphs, around 800 characters of prose. \
                     Describe the scene, the action, and what the player sees next. \
                     Room arrivals get atmosphere and exits. Combat gets kinetic beats. \
                     Dialogue gets voice and personality. Vary length by moment.\n\
                     </length-limit>"
                }
                NarratorVerbosity::Verbose => {
                    "<length-limit>\n\
                     Target: 2-4 paragraphs, around 1200 characters of prose. \
                     Rich atmosphere, sensory detail, NPC personality. Let scenes breathe. \
                     Big moments (arrivals, reveals, combat starts) get the full treatment. \
                     Quieter turns can be shorter — vary the rhythm.\n\
                     </length-limit>"
                }
            };
            builder.add_section(PromptSection::new(
                "narrator_verbosity",
                content,
                AttentionZone::Recency,
                SectionCategory::Guardrail,
            ));
        }

        // First-turn opening constraint (Recency zone, Full tier only).
        // The opening narration after character creation tends to run long (~10 paragraphs).
        // This tightens it to 3-4 short paragraphs that set the scene and prompt action.
        if is_full {
            builder.add_section(PromptSection::new(
                "opening_scene_constraint",
                "<opening-scene>\n\
                 This is the OPENING SCENE — the player's first moment in the world.\n\
                 Set the scene in 3-4 SHORT paragraphs maximum:\n\
                 1. Where they are (one vivid detail, not a catalogue).\n\
                 2. What's immediately happening around them.\n\
                 3. One sensory hook — sound, smell, weather.\n\
                 4. End with a prompt for their first action (a question, a choice, a threat).\n\
                 Do NOT write a novel opening. Do NOT describe the world's history. \
                 Do NOT list every feature of the environment. Drop the player IN and \
                 let them explore. Under 500 characters of prose total.\n\
                 MANDATORY: Your game_patch MUST include a visual_scene for this opening \
                 turn — it is the first illustration the player sees. Use tier \
                 \"landscape\" and describe the opening vista.\n\
                 </opening-scene>",
                AttentionZone::Recency,
                SectionCategory::Guardrail,
            ));
        }

        // Narrator vocabulary instruction (Late zone, Full tier only — stable across session)
        if is_full {
            use sidequest_protocol::NarratorVocabulary;
            let content = match context.narrator_vocabulary {
                NarratorVocabulary::Accessible => {
                    "[NARRATION VOCABULARY]\n\
                     Use simple, direct language. Prefer common words over obscure \
                     ones. Keep sentences short and clear. Aim for approximately \
                     8th-grade reading level. No archaic constructions or elaborate \
                     metaphors."
                }
                NarratorVocabulary::Literary => {
                    "[NARRATION VOCABULARY]\n\
                     Use rich but clear prose. Employ varied vocabulary and literary \
                     devices where they serve the narrative. Balance elegance with \
                     accessibility — vivid but not purple."
                }
                NarratorVocabulary::Epic => {
                    "[NARRATION VOCABULARY]\n\
                     Use elevated, archaic, or mythic diction. Embrace elaborate \
                     sentence structures, rare words, and poetic constructions. \
                     Channel the cadence of sagas, epics, and high fantasy prose. \
                     Unrestricted complexity."
                }
            };
            builder.add_section(PromptSection::new(
                "narrator_vocabulary",
                content,
                AttentionZone::Late,
                SectionCategory::Format,
            ));
        }

        // Player action section (Recency zone)
        builder.add_section(PromptSection::new(
            "player_action",
            format!("{} says: {}", context.character_name, action),
            AttentionZone::Recency,
            SectionCategory::Action,
        ));

        let section_count = builder.section_count();
        let _pb_guard = tracing::info_span!(
            "turn.agent_llm.prompt_build",
            section_count = section_count as u64,
        )
        .entered();
        let zone_breakdown = builder.zone_breakdown();
        let prompt_text = builder.compose();
        let allowed_tools = self.narrator_allowed_tools();

        NarratorPromptResult {
            prompt_text,
            zone_breakdown,
            script_tools_injected,
            allowed_tools,
            intent_route: route,
            env_vars,
        }
    }
}

impl GameService for Orchestrator {
    fn get_snapshot(&self) -> serde_json::Value {
        serde_json::Value::Object(serde_json::Map::new())
    }

    fn reset_narrator_session_for_connect(&self) {
        self.reset_narrator_session();
    }

    fn process_action(&self, action: &str, context: &TurnContext) -> ActionResult {
        let span = tracing::info_span!(
            "orchestrator.process_action",
            action_len = action.len(),
            intent = tracing::field::Empty,
            agent = tracing::field::Empty,
            is_degraded = tracing::field::Empty,
            prompt_tier = tracing::field::Empty,
        );
        let _guard = span.enter();

        // Build prompt via extracted method (story 15-27: testable prompt assembly).
        // ADR-066: Use Delta tier on resumed sessions — static context already in history.
        // Story 30-2: Use select_prompt_tier() for genre switch detection.
        let prompt_tier = self.select_prompt_tier(context);
        let prompt_tier_str = match prompt_tier {
            NarratorPromptTier::Full => "full",
            NarratorPromptTier::Delta => "delta",
        };
        span.record("prompt_tier", prompt_tier_str);
        let prompt_result = self.build_narrator_prompt_tiered(action, context, prompt_tier);
        let prompt = prompt_result.prompt_text;
        let prompt_zone_breakdown = prompt_result.zone_breakdown;
        let allowed_tools = prompt_result.allowed_tools;
        let env_vars = prompt_result.env_vars;

        // OTEL: report which script tools were injected into this turn's prompt
        if !prompt_result.script_tools_injected.is_empty() {
            let _tools_span = tracing::info_span!(
                "script_tool.prompt_injected",
                tools = %prompt_result.script_tools_injected.join(","),
                count = prompt_result.script_tools_injected.len(),
            )
            .entered();
        }

        // Reuse intent classification from build_narrator_prompt (ADR-067: state inference, no LLM).
        let route = prompt_result.intent_route;
        span.record("intent", route.intent().to_string().as_str());
        span.record("agent", route.agent_name());
        info!(
            intent = %route.intent(),
            source = "state_inference",
            "unified_narrator.intent_inferred"
        );

        // Generate a unique session ID for the tool call sidecar file.
        // ADR-059: Sidecar mechanism removed. Monster Manual handles pre-generation.
        info!(action = %action, "Invoking Claude CLI for narration");

        let intent_str = route.intent().to_string();
        let agent_str = route.agent_name().to_string();

        // ADR-066: Opus via persistent session (--resume). Session established on
        // first call, resumed on subsequent turns. Opus with 1M context + server-side
        // caching gives ~6s turns after warm-up vs ~22s with per-turn Sonnet rebuild.
        let narrator_model = "opus";
        let has_tools = !allowed_tools.is_empty();

        // Read current session ID (None on first turn)
        let current_session_id = self.narrator_session_id.lock().unwrap().clone();
        let is_first_turn = current_session_id.is_none();

        // On first turn, the full prompt IS the system prompt + first action.
        // On subsequent turns, only the action + state delta is sent.
        let system_prompt_for_establish = if is_first_turn {
            Some(prompt.clone())
        } else {
            None
        };
        // For resumed turns, send just the action context (state delta + player action).
        // The system prompt and prior conversation are already in the session.
        let send_prompt = if is_first_turn {
            // First turn: prompt is the action part; system prompt carries the context
            action.to_string()
        } else {
            prompt.clone()
        };

        let inference_span = tracing::info_span!(
            "turn.agent_llm.inference",
            model = narrator_model,
            prompt_len = send_prompt.len(),
            tool_use = has_tools,
            persistent_session = true,
            is_first_turn = is_first_turn,
        );
        let call_start = std::time::Instant::now();
        let send_result = {
            let _inf_guard = inference_span.enter();
            self.client.send_with_session(
                &send_prompt,
                narrator_model,
                current_session_id.as_deref(),
                system_prompt_for_establish.as_deref(),
                &allowed_tools,
                &env_vars,
            )
        };

        // Store session ID from response (first turn establishes, subsequent echo it back)
        // Story 30-2: Also record the genre that established this session for switch detection.
        if let Ok(ref response) = send_result {
            if let Some(ref sid) = response.session_id {
                let mut lock = self.narrator_session_id.lock().unwrap();
                if lock.is_none() {
                    tracing::info!(session_id = %sid, "narrator.session_established — persistent Opus session created");
                    // Record the genre for this session so we can detect genre switches
                    if let Some(ref genre) = context.genre {
                        *self.session_genre.lock().unwrap() = Some(genre.clone());
                    }
                }
                *lock = Some(sid.clone());
            }
        }
        match send_result {
            Ok(response) => {
                let raw_response = &response.text;
                // Parse narrator response — strip fences, return prose
                let extraction_span = tracing::info_span!(
                    "turn.agent_llm.parse_response",
                    narration_len = raw_response.len(),
                );
                let _ext_guard = extraction_span.enter();
                let extraction = extract_structured_from_response(raw_response);
                if !extraction.footnotes.is_empty() {
                    info!(
                        count = extraction.footnotes.len(),
                        new_count = extraction.footnotes.iter().filter(|f| f.is_new).count(),
                        "rag.footnotes_extracted"
                    );
                }
                if !extraction.items_gained.is_empty() {
                    info!(
                        count = extraction.items_gained.len(),
                        "rag.items_gained_extracted"
                    );
                }
                // Story 28-8: Log confrontation initiation if present.
                if let Some(ref ctype) = extraction.confrontation {
                    info!(confrontation_type = %ctype, "encounter.confrontation_initiated");
                }

                // Story 28-6: Extract beat_selections from game_patch block.
                // The legacy combat/chase patch extraction pathways were removed
                // in story 28-9 — beat_selections is the unified replacement.
                let beat_selections = extraction.beat_selections.clone();
                if !beat_selections.is_empty() {
                    for bs in &beat_selections {
                        info!(
                            actor = %bs.actor,
                            beat_id = %bs.beat_id,
                            target = ?bs.target,
                            "encounter.agent_beat_selection"
                        );
                    }
                }

                let agent_duration_ms = call_start.elapsed().as_millis() as u64;
                span.record("is_degraded", false);

                // ADR-059: sidecar tool results removed. Monster Manual handles pre-generation.
                // assemble_turn still merges extraction + preprocessor with default tool results.
                let tool_results = crate::tools::assemble_turn::ToolCallResults::default();

                if extraction.action_rewrite.is_none() {
                    warn!("action_rewrite absent from extraction — using default (empty rewrite)");
                }
                if extraction.action_flags.is_none() {
                    warn!("action_flags absent from extraction — using default (all flags false)");
                }
                let rewrite = extraction.action_rewrite.clone().unwrap_or_default();
                let flags = extraction.action_flags.clone().unwrap_or_default();
                let mut base = assemble_turn(extraction, rewrite, flags, tool_results);

                // Strip any residual JSON fence blocks from narration prose.
                // The narrator should produce pure prose, but fences can appear
                // in edge cases (e.g., tool output leaking into narration text).
                base.narration = strip_json_fence(&base.narration);

                info!(
                    len = base.narration.len(),
                    duration_ms = agent_duration_ms,
                    "Claude CLI returned narration"
                );

                // Orchestrator overrides fields assemble_turn doesn't know about
                ActionResult {
                    beat_selections,
                    is_degraded: false,
                    classified_intent: Some(intent_str),
                    agent_name: Some(agent_str),
                    agent_duration_ms: Some(agent_duration_ms),
                    token_count_in: response.input_tokens.map(|v| v as usize),
                    token_count_out: response.output_tokens.map(|v| v as usize),
                    zone_breakdown: Some(prompt_zone_breakdown),
                    prompt_tier: prompt_tier_str.to_string(),
                    prompt_text: Some(prompt.clone()),
                    raw_response_text: Some(response.text.clone()),
                    ..base
                }
            }
            Err(e) => {
                let agent_duration_ms = call_start.elapsed().as_millis() as u64;
                tracing::error!(
                    agent = %agent_str,
                    duration_ms = agent_duration_ms,
                    error = %e,
                    "CLAUDE CLI FAILED — returning degraded response (ADR-005)"
                );
                // ADR-005: graceful degradation. Return a degraded narration
                // so the game loop continues. The player sees a brief pause
                // message instead of a disconnection.
                let degraded_narration = format!(
                    "**{}**\n\nThe world holds its breath for a moment... \
                     something shifts in the distance, but the moment passes.",
                    context.current_location
                );
                ActionResult {
                    narration: degraded_narration,
                    beat_selections: vec![],
                    is_degraded: true,
                    classified_intent: Some(intent_str),
                    agent_name: Some(agent_str),
                    footnotes: vec![],
                    items_gained: vec![],
                    npcs_present: vec![],
                    quest_updates: HashMap::new(),
                    agent_duration_ms: Some(agent_duration_ms),
                    token_count_in: None,
                    token_count_out: None,
                    visual_scene: None,
                    scene_mood: None,
                    personality_events: vec![],
                    scene_intent: None,
                    resource_deltas: HashMap::new(),
                    zone_breakdown: Some(prompt_zone_breakdown),
                    prompt_tier: prompt_tier_str.to_string(),
                    lore_established: None,
                    merchant_transactions: vec![],
                    sfx_triggers: vec![],
                    action_rewrite: None,
                    action_flags: None,
                    confrontation: None,
                    location: None,
                    prompt_text: Some(prompt.clone()),
                    raw_response_text: None,
                }
            }
        }
    }
}

// ============================================================================
// Combat patch: strip JSON fence from prose after extraction
// ============================================================================

/// Extract and deserialize JSON from a markdown fenced code block.
///
/// Used for responses that may wrap their JSON output in ```json ... ``` or ```game_patch ... ``` fences.
fn extract_fenced_json<T: serde::de::DeserializeOwned>(
    input: &str,
) -> Result<T, serde_json::Error> {
    // Try ```json ... ``` first
    if let Some(start) = input.find("```json") {
        if let Some(end) = input[start + 7..].find("```") {
            let json_str = input[start + 7..start + 7 + end].trim();
            return serde_json::from_str(json_str);
        }
    }
    // Try bare ``` ... ```, but skip past any ```json opener we already tried
    let search_start = input.find("```json").map(|s| s + 7).unwrap_or(0);
    if let Some(rel_start) = input[search_start..].find("```") {
        let start = search_start + rel_start;
        if let Some(end) = input[start + 3..].find("```") {
            let json_str = input[start + 3..start + 3 + end].trim();
            return serde_json::from_str(json_str);
        }
    }
    // No fenced JSON found — return a clear error
    serde_json::from_str::<T>("")
}

/// Remove fenced code blocks (```json, ```game_patch, or bare ```) from narration
/// so the player sees clean prose.
fn strip_json_fence(input: &str) -> String {
    let re = regex::Regex::new(r"(?s)```(?:json|game_patch)?\s*\n[\s\S]*?\n```").unwrap();
    re.replace(input, "").trim().to_string()
}

// ============================================================================
// ADR-059 follow-up: game_patch structured extraction
//
// The narrator emits a ```game_patch { ... }``` block every turn. ADR-057 said
// structured data would come from tool call sidecars; ADR-059 removed sidecars.
// This extraction parses the game_patch block directly instead of returning
// empty defaults.
// ============================================================================

/// All structured fields the narrator may emit in a ```game_patch``` block.
/// Every field has `#[serde(default)]` so partial blocks parse cleanly.
#[derive(Debug, Default, serde::Deserialize)]
struct GamePatchExtraction {
    #[serde(default)]
    footnotes: Vec<sidequest_protocol::Footnote>,
    #[serde(default)]
    items_gained: Vec<sidequest_protocol::ItemGained>,
    /// `npcs_present` and `npcs_met` are both valid labels from the narrator.
    #[serde(default, alias = "npcs_met")]
    npcs_present: Vec<NpcMention>,
    #[serde(default)]
    quest_updates: HashMap<String, String>,
    #[serde(default)]
    visual_scene: Option<VisualScene>,
    /// `scene_mood` and `mood` are both valid labels from the narrator.
    #[serde(default, alias = "mood")]
    scene_mood: Option<String>,
    #[serde(default)]
    personality_events: Vec<PersonalityEvent>,
    #[serde(default)]
    scene_intent: Option<String>,
    #[serde(default)]
    resource_deltas: HashMap<String, f64>,
    #[serde(default)]
    lore_established: Option<Vec<String>>,
    #[serde(default)]
    merchant_transactions: Vec<MerchantTransactionExtracted>,
    #[serde(default)]
    sfx_triggers: Vec<String>,
    #[serde(default)]
    action_rewrite: Option<ActionRewrite>,
    #[serde(default)]
    action_flags: Option<ActionFlags>,
    // Story 28-6: beat_selections replaces in_combat/hp_changes/turn_order/drama_weight.
    // The narrator selects beats from the active ConfrontationDef.
    #[serde(default)]
    beat_selections: Vec<BeatSelection>,
    /// Story 28-8: Narrator signals encounter start by naming a confrontation type
    /// from the genre pack's ConfrontationDefs (e.g., "combat", "standoff", "chase").
    /// The server creates a StructuredEncounter via from_confrontation_def().
    #[serde(default)]
    confrontation: Option<String>,
    /// Location name from the narrator's game_patch JSON (fallback for header extraction).
    #[serde(default)]
    location: Option<String>,
}

/// Extract and parse the ```game_patch``` block from a raw narrator response.
///
/// Tries ```game_patch first, then falls back to ```json, then returns defaults.
fn extract_game_patch(raw: &str) -> GamePatchExtraction {
    // Primary: ```game_patch ... ```
    if let Some(start) = raw.find("```game_patch") {
        let after_label = start + "```game_patch".len();
        if let Some(rel_end) = raw[after_label..].find("```") {
            let json_str = raw[after_label..after_label + rel_end].trim();
            match serde_json::from_str::<GamePatchExtraction>(json_str) {
                Ok(patch) => return patch,
                Err(e) => {
                    tracing::warn!(error = %e, "game_patch block found but failed to parse");
                }
            }
        }
    }
    // Fallback: ```json ... ```
    if let Ok(patch) = extract_fenced_json::<GamePatchExtraction>(raw) {
        return patch;
    }
    // No structured data — return empty defaults
    GamePatchExtraction::default()
}

// ============================================================================
// Story 9-11: Footnote extraction from narrator response
// ============================================================================

/// Serde model for the narrator's structured JSON output block.
/// An NPC mentioned in the narrator's structured output.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct NpcMention {
    /// Full canonical name (e.g., "Toggler Copperjaw", not "Toggler").
    pub name: String,
    /// Pronouns (e.g., "he/him", "she/her", "they/them").
    #[serde(default)]
    pub pronouns: String,
    /// Role in one or two words (e.g., "blacksmith", "faction leader").
    #[serde(default)]
    pub role: String,
    /// Brief physical description (only for new introductions).
    #[serde(default)]
    pub appearance: String,
    /// True if this NPC appears for the first time this turn.
    #[serde(default)]
    pub is_new: bool,
}

/// Visual scene description extracted from narrator JSON block.
/// Eliminates the separate LLM call for image subject extraction.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct VisualScene {
    /// The image subject description for the renderer.
    pub subject: String,
    /// Render tier (e.g., "portrait", "scene", "vignette").
    #[serde(default)]
    pub tier: String,
    /// Visual mood keyword (e.g., "tense", "serene", "chaotic").
    #[serde(default)]
    pub mood: String,
    /// Additional style/content tags for the renderer.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// An OCEAN personality event for an NPC extracted from narrator JSON block.
/// The `event_type` field maps directly to the game's PersonalityEvent enum.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PersonalityEvent {
    /// The NPC's canonical name.
    pub npc: String,
    /// Typed event — one of: betrayal, near_death, victory, defeat, social_bonding.
    /// Deserialized directly from the narrator's JSON block.
    pub event_type: sidequest_game::PersonalityEvent,
    /// Optional free-text description (for OTEL telemetry, not for classification).
    #[serde(default)]
    pub description: String,
}

/// A merchant transaction extracted from the narrator's JSON block.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct MerchantTransactionExtracted {
    /// "buy" or "sell" (from player's perspective).
    #[serde(rename = "type")]
    pub transaction_type: String,
    /// Item identifier (snake_case name matching inventory).
    pub item_id: String,
    /// Merchant NPC name.
    pub merchant: String,
}

/// Action rewrite from inline preprocessor (narrator/creature_smith JSON block).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct ActionRewrite {
    /// Player-facing rewrite ("you ...").
    #[serde(default)]
    pub you: String,
    /// Third-person rewrite ("Kael ...").
    #[serde(default)]
    pub named: String,
    /// Distilled intent label.
    #[serde(default)]
    pub intent: String,
}

/// Relevance flags from inline preprocessor (narrator/creature_smith JSON block).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct ActionFlags {
    /// True if the action is a coercive/power-claim style move.
    #[serde(default)]
    pub is_power_grab: bool,
    /// True if the action references inventory items.
    #[serde(default)]
    pub references_inventory: bool,
    /// True if the action references an NPC by name.
    #[serde(default)]
    pub references_npc: bool,
    /// True if the action references a character ability.
    #[serde(default)]
    pub references_ability: bool,
    /// True if the action references a location.
    #[serde(default)]
    pub references_location: bool,
}

/// Extracted structured data from a narrator response.
pub struct NarratorExtraction {
    /// The cleaned prose text with JSON block removed.
    pub prose: String,
    /// Lore footnotes extracted from the narrator response.
    pub footnotes: Vec<sidequest_protocol::Footnote>,
    /// Items the player gained during this turn.
    pub items_gained: Vec<sidequest_protocol::ItemGained>,
    /// NPCs mentioned or present in the narration.
    pub npcs_present: Vec<NpcMention>,
    /// Quest state changes keyed by quest name.
    pub quest_updates: HashMap<String, String>,
    /// Visual scene description for image generation (eliminates separate LLM call).
    pub visual_scene: Option<VisualScene>,
    /// Scene mood tag for atmosphere/music selection.
    pub scene_mood: Option<String>,
    /// OCEAN personality events for NPCs present this turn.
    pub personality_events: Vec<PersonalityEvent>,
    /// High-level narrative intent of this scene.
    pub scene_intent: Option<String>,
    /// Resource deltas extracted from narrator JSON block (story 16-1).
    pub resource_deltas: HashMap<String, f64>,
    /// Lore fragments established this turn (story 15-7).
    pub lore_established: Option<Vec<String>>,
    /// Merchant transactions extracted from narrator JSON block.
    pub merchant_transactions: Vec<MerchantTransactionExtracted>,
    /// SFX trigger IDs from the narrator's scene analysis.
    pub sfx_triggers: Vec<String>,
    /// Inline preprocessor: action rewrite (eliminates separate Haiku call).
    pub action_rewrite: Option<ActionRewrite>,
    /// Inline preprocessor: relevance flags.
    pub action_flags: Option<ActionFlags>,
    /// Beat selections from narrator output (story 28-6).
    pub beat_selections: Vec<BeatSelection>,
    /// Confrontation type to initiate (story 28-8). When the narrator emits
    /// `"confrontation": "combat"`, the server creates a StructuredEncounter.
    pub confrontation: Option<String>,
    /// Location name from game_patch JSON (fallback for header extraction).
    pub location: Option<String>,
}

/// Extract the narrator's prose and all structured fields from a raw response.
///
/// The narrator emits a ```game_patch { ... }``` block every turn containing
/// footnotes, items, NPCs, mood, etc. This function parses that block and maps
/// it to `NarratorExtraction`, then strips the fence from the returned prose.
fn extract_structured_from_response(raw: &str) -> NarratorExtraction {
    let span = tracing::info_span!("rag.prose_cleanup", raw_len = raw.len());
    let _guard = span.enter();

    // Parse game_patch before stripping (strip_json_fence returns a new String,
    // so `raw` is still intact here).
    let patch = extract_game_patch(raw);

    // Log extraction counts for OTEL visibility.
    info!(
        footnotes = patch.footnotes.len(),
        items_gained = patch.items_gained.len(),
        npcs_present = patch.npcs_present.len(),
        quest_updates = patch.quest_updates.len(),
        personality_events = patch.personality_events.len(),
        sfx_triggers = patch.sfx_triggers.len(),
        resource_deltas = patch.resource_deltas.len(),
        has_visual_scene = patch.visual_scene.is_some(),
        has_scene_mood = patch.scene_mood.is_some(),
        has_lore = patch.lore_established.is_some(),
        has_action_rewrite = patch.action_rewrite.is_some(),
        has_action_flags = patch.action_flags.is_some(),
        beat_selections = patch.beat_selections.len(),
        confrontation = ?patch.confrontation,
        has_location = patch.location.is_some(),
        "game_patch.extracted"
    );

    let prose = strip_json_fence(raw);

    NarratorExtraction {
        prose,
        footnotes: patch.footnotes,
        items_gained: patch.items_gained,
        npcs_present: patch.npcs_present,
        quest_updates: patch.quest_updates,
        visual_scene: patch.visual_scene,
        scene_mood: patch.scene_mood,
        personality_events: patch.personality_events,
        scene_intent: patch.scene_intent,
        resource_deltas: patch.resource_deltas,
        lore_established: patch.lore_established,
        merchant_transactions: patch.merchant_transactions,
        sfx_triggers: patch.sfx_triggers,
        action_rewrite: patch.action_rewrite,
        action_flags: patch.action_flags,
        beat_selections: patch.beat_selections,
        confrontation: patch.confrontation,
        location: patch.location,
    }
}

// ============================================================================
// Story 2-5: Turn loop types
// ============================================================================

/// State flags passed to intent classification for state-based overrides.
#[derive(Debug, Clone, Default)]
pub struct TurnContext {
    /// Whether the game is currently in active combat.
    pub in_combat: bool,
    /// Whether the game is currently in an active chase.
    pub in_chase: bool,
    /// Whether ANY encounter is active (combat, chase, standoff, negotiation, etc.).
    /// Broader than in_combat/in_chase — covers all ConfrontationDef types.
    pub in_encounter: bool,
    /// Serialized game state summary for grounding narration.
    pub state_summary: Option<String>,
    /// Per-session narrator verbosity setting (concise/standard/verbose).
    pub narrator_verbosity: sidequest_protocol::NarratorVerbosity,
    /// Per-session narrator vocabulary setting (accessible/literary/epic).
    pub narrator_vocabulary: sidequest_protocol::NarratorVocabulary,
    /// Trope beat directives from the previous turn's fired beats.
    /// Injected into the Early attention zone so the narrator weaves them in.
    pub pending_trope_context: Option<String>,
    /// Active trope summary for background context (all agents, Valley zone).
    pub active_trope_summary: Option<String>,
    /// Genre slug for the current session (e.g., "mutant_wasteland").
    /// Required for script tool prompt injection — tools need the genre to call the binary.
    pub genre: Option<String>,
    /// Available SFX IDs from the genre pack's sfx_library.
    /// Injected into the narrator prompt so it knows what SFX to pick from.
    pub available_sfx: Vec<String>,
    /// NPC registry entries for merchant detection (story 15-16).
    /// Populated from GameSnapshot.npc_registry by the server dispatch loop.
    pub npc_registry: Vec<NpcRegistryEntry>,
    /// Full NPC structs for merchant context injection (story 15-16).
    /// Only NPCs at the current location are needed, but all are passed
    /// for simplicity — inject_merchant_context filters by location.
    pub npcs: Vec<Npc>,
    /// Player's current location for merchant context injection (story 15-16).
    pub current_location: String,
    /// Hierarchical world graph for lore filtering (story 23-4).
    /// When present, LoreFilter gates Valley zone lore injection by graph distance.
    pub world_graph: Option<sidequest_genre::WorldGraph>,
    /// History chapters from genre pack for progressive world materialization (story 15-18).
    /// Filtered by campaign_maturity in the prompt builder via WorldBuilderAgent.
    pub history_chapters: Vec<sidequest_game::world_materialization::HistoryChapter>,
    /// Current campaign maturity for world materialization filtering (story 15-18).
    pub campaign_maturity: sidequest_game::world_materialization::CampaignMaturity,
    /// Character name of the acting player (multiplayer action attribution).
    pub character_name: String,
    /// Genre-specific prompt templates from prompts.yaml.
    /// Injected contextually: narrator voice on Full tier, combat/npc/world_state per state.
    pub genre_prompts: Option<sidequest_genre::Prompts>,
}

/// Result of processing a player action through the full turn loop.
#[derive(Debug, Clone)]
pub struct TurnResult {
    /// The narrative text produced by the agent.
    pub narration: String,
    /// Optional state delta for the client.
    pub state_delta: Option<HashMap<String, serde_json::Value>>,
    /// Combat events that occurred during this action.
    pub combat_events: Vec<String>,
    /// Whether this is a degraded response (e.g., from agent timeout).
    pub is_degraded: bool,
    /// Which agent produced this result.
    pub agent_used: AgentKind,
    /// Drama-aware delivery mode for text reveal (Story 5-7).
    pub delivery_mode: DeliveryMode,
}

/// Typed agent selection — replaces string-based agent keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentKind {
    /// Primary narrator for exploration and general narration.
    Narrator,
    /// Combat specialist — generates encounters, manages combat flow.
    CreatureSmith,
    /// NPC dialogue and ensemble scenes.
    Ensemble,
    /// Chase sequence orchestrator.
    Dialectician,
    /// Post-turn world state updates.
    WorldBuilder,
    /// Trope lifecycle management.
    Troper,
    /// Theme and atmosphere resonance.
    Resonator,
    /// Intent classification (LLM-based).
    IntentRouter,
}

/// Inject merchant context into the prompt builder when appropriate.
///
/// Checks the NPC registry for merchants at the player's current location.
/// For each merchant found, calls `format_merchant_context()` and adds the
/// result as a Valley-zone section. Only injects for Exploration and Dialogue
/// intents — combat and chase don't need merchant wares in the prompt.
///
/// Emits a `merchant.context_injected` OTEL span per merchant for GM panel visibility.
pub fn inject_merchant_context(
    builder: &mut ContextBuilder,
    registry: &[NpcRegistryEntry],
    npcs: &[Npc],
    intent: Intent,
    current_location: &str,
) {
    // Only inject for Exploration and Dialogue intents
    if !matches!(intent, Intent::Exploration | Intent::Dialogue) {
        return;
    }

    // Find merchants at the player's current location
    let merchant_entries: Vec<&NpcRegistryEntry> = registry
        .iter()
        .filter(|entry| entry.role.contains("merchant") && entry.location == current_location)
        .collect();

    for entry in merchant_entries {
        // Look up the full NPC to get inventory and disposition
        let Some(npc) = npcs.iter().find(|n| n.core.name.as_str() == entry.name) else {
            continue;
        };

        let item_count = npc.core.inventory.item_count();
        let context_text =
            format_merchant_context(&entry.name, &npc.core.inventory, &npc.disposition);

        // OTEL span for GM panel
        let _span = tracing::info_span!(
            "merchant.context_injected",
            merchant_name = %entry.name,
            item_count = item_count,
        )
        .entered();

        builder.add_section(PromptSection::new(
            "merchant_context",
            context_text,
            AttentionZone::Valley,
            SectionCategory::State,
        ));
    }
}

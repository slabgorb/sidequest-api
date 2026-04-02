//! Orchestrator — state machine that sequences agents and manages game state.
//!
//! Port lesson #1: Server talks to GameService trait, never game internals.
//! ADR-010: Intent-based agent routing via LLM classification.

use std::collections::HashMap;

use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::agent::Agent;
use crate::tools::assemble_turn::assemble_turn;
use crate::tools::tool_call_parser::parse_tool_results;
use crate::agents::creature_smith::CreatureSmithAgent;
use crate::agents::dialectician::DialecticianAgent;
use crate::agents::ensemble::EnsembleAgent;
use crate::agents::intent_router::{Intent, IntentRoute, IntentRouter};
use crate::agents::narrator::NarratorAgent;
use crate::agents::troper::TroperAgent;
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
    /// Typed combat patch extracted from creature_smith response.
    pub combat_patch: Option<crate::patches::CombatPatch>,
    /// Typed chase patch extracted from dialectician response.
    pub chase_patch: Option<crate::patches::ChasePatch>,
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
    /// Two-tier intent classifier (ADR-032: Haiku → narrator fallback).
    intent_router: IntentRouter,
    /// Specialist agents — dispatched by intent classification.
    narrator: NarratorAgent,
    creature_smith: CreatureSmithAgent,
    ensemble: EnsembleAgent,
    dialectician: DialecticianAgent,
    /// Pacing engine — tracks drama weight across combat turns (Story 5-7).
    tension_tracker: TensionTracker,
    /// Genre-tunable pacing breakpoints (Story 5-7).
    drama_thresholds: DramaThresholds,
    /// Trope beat injection agent (ADR-018).
    troper: TroperAgent,
    /// SOUL.md principles — filtered per agent via `<agents>` tags and injected in the Early zone.
    soul_data: Option<crate::prompt_framework::SoulData>,
    /// Script tool configurations (ADR-056). Keyed by tool name.
    script_tools: HashMap<String, ScriptToolConfig>,
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
    pub fn new_with_otel(watcher_tx: mpsc::Sender<TurnRecord>, otel_endpoint: Option<String>) -> Self {
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
            creature_smith: CreatureSmithAgent::new(),
            ensemble: EnsembleAgent::new(),
            dialectician: DialecticianAgent::new(),
            tension_tracker: TensionTracker::new(),
            drama_thresholds: DramaThresholds::default(),
            troper: TroperAgent::new(),
            soul_data,
            script_tools: HashMap::new(),
        }
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
        self.script_tools
            .values()
            .map(|cfg| format!("Bash({}:*)", cfg.binary_path))
            .collect()
    }

    /// Access the tension tracker (pacing engine).
    pub fn tension(&self) -> &TensionTracker {
        &self.tension_tracker
    }

    /// Access the drama thresholds (genre-tunable pacing breakpoints).
    pub fn drama_thresholds(&self) -> &DramaThresholds {
        &self.drama_thresholds
    }

    /// Mutable access to the Troper agent for loading fired beats.
    pub fn troper_mut(&mut self) -> &mut TroperAgent {
        &mut self.troper
    }

    /// Read access to the Troper agent.
    pub fn troper(&self) -> &TroperAgent {
        &self.troper
    }

    /// Build the narrator prompt and tool configuration without invoking the LLM.
    ///
    /// Extracted from `process_action()` (story 15-27) so prompt content — including
    /// script tool sections — can be tested independently. Runs intent classification
    /// internally to determine which agent's system prompt to use.
    ///
    /// Returns the composed prompt text, zone breakdown, injected script tool names,
    /// and the `--allowedTools` specs for the Claude CLI.
    pub fn build_narrator_prompt(&self, action: &str, context: &TurnContext) -> NarratorPromptResult {
        let route = self.intent_router.classify(action, context);

        let mut builder = ContextBuilder::new();
        let mut script_tools_injected: Vec<String> = Vec::new();

        // Agent identity section (Primacy zone)
        match route.agent_name() {
            "creature_smith" => self.creature_smith.build_context(&mut builder),
            "ensemble" => self.ensemble.build_context(&mut builder),
            "dialectician" => self.dialectician.build_context(&mut builder),
            _ => self.narrator.build_context(&mut builder),
        };

        // SOUL principles (Early zone — high attention, after identity, before state)
        // Filtered per agent via <agents> tags in SOUL.md.
        if let Some(ref soul) = self.soul_data {
            let filtered = soul.as_prompt_text_for(route.agent_name());
            if !filtered.is_empty() {
                builder.add_section(PromptSection::new(
                    "soul_principles",
                    format!("## Guiding Principles\n{}", filtered),
                    AttentionZone::Early,
                    SectionCategory::Soul,
                ));
            }
        }

        // Trope beat directives (Early zone)
        if let Some(ref beats) = context.pending_trope_context {
            let _trope_span = tracing::info_span!(
                "orchestrator.trope_beat_injection",
                beats_injected = 1u64,
            )
            .entered();
            builder.add_section(PromptSection::new(
                "trope_beat_directives",
                beats.clone(),
                AttentionZone::Early,
                SectionCategory::State,
            ));
        }

        // Script tool instructions (Valley zone — available tools + commands)
        if let Some(ref genre) = context.genre {
            for (tool_name, cfg) in &self.script_tools {
                let tool_section = match tool_name.as_str() {
                    "encountergen" => format!(
                        "[ENCOUNTER GENERATOR]\n\
                         Generate enemy stat blocks from genre pack data.\n\n\
                         Command:\n\
                         ```\n\
                         {} --genre-packs-path {} --genre {} [options]\n\
                         ```\n\n\
                         | Flag | Required | Description |\n\
                         |------|----------|-------------|\n\
                         | --tier | No | Power tier 1-4 (default: random 1-3) |\n\
                         | --count | No | Number of enemies (default: 1) |\n\
                         | --class | No | Character class (e.g., Mutant, Scavenger) |\n\
                         | --culture | No | Culture for name generation |\n\
                         | --archetype | No | Archetype (e.g., \"Wasteland Trader\") |\n\
                         | --role | No | Role description (e.g., \"ambush predator\") |\n\
                         | --context | No | Scene context for visual prompt |\n\n\
                         When to call: any time new enemies enter the scene.\n\
                         Pick flags based on narrative context — use --culture for the local faction, \
                         --tier for the threat level, --role for the enemy's purpose in the scene.\n\n\
                         Output: JSON with enemies[].{{name, class, level, hp, abilities, weaknesses, \
                         stat_scores, visual_prompt, ...}}\n\n\
                         Checklist after calling:\n\
                         - [ ] Use the generated name in your narration\n\
                         - [ ] Reference abilities from the abilities list (not invented ones)\n\
                         - [ ] Include the enemy in npcs_present with is_new: true\n\
                         - [ ] Set hp_changes in combat patch using the generated HP as the baseline\n\
                         - [ ] The visual_prompt field goes to image generation automatically",
                        cfg.binary_path, cfg.genre_packs_path, genre,
                    ),
                    "namegen" => format!(
                        "[NPC GENERATOR]\n\
                         Generate NPC identity from genre pack data.\n\n\
                         Command:\n\
                         ```\n\
                         {} --genre-packs-path {} --genre {} [options]\n\
                         ```\n\n\
                         | Flag | Required | Description |\n\
                         |------|----------|-------------|\n\
                         | --culture | No | Culture/faction name |\n\
                         | --archetype | No | Archetype name |\n\
                         | --gender | No | male, female, nonbinary |\n\
                         | --role | No | Role override |\n\
                         | --description | No | Physical description hints |\n\n\
                         When to call: any time a new NPC appears (is_new: true).\n\
                         Pick --culture based on where the scene takes place.\n\n\
                         Output: JSON with {{name, pronouns, culture, role, appearance, personality, \
                         dialogue_quirks, history, ocean, inventory, trope_connections}}\n\n\
                         Checklist after calling:\n\
                         - [ ] Use the generated name exactly\n\
                         - [ ] Use dialogue_quirks to flavor their speech\n\
                         - [ ] Include in npcs_present with the generated details\n\
                         - [ ] Reference their role and appearance in narration",
                        cfg.binary_path, cfg.genre_packs_path, genre,
                    ),
                    "loadoutgen" => format!(
                        "[STARTING LOADOUT GENERATOR]\n\
                         Generate starting equipment and currency for a character.\n\n\
                         Command:\n\
                         ```\n\
                         {} --genre-packs-path {} --genre {} --class <class_name>\n\
                         ```\n\n\
                         | Flag | Required | Description |\n\
                         |------|----------|-------------|\n\
                         | --class | Yes | Character class or archetype name |\n\
                         | --tier | No | Power tier 1-4 (default: 1) |\n\n\
                         When to call: at character creation completion or session start, \
                         when introducing the character's starting gear.\n\n\
                         Output: JSON with {{class, currency_name, starting_gold, equipment[], \
                         narrative_hook, total_value}}\n\
                         Each equipment item has: id, name, description, category, value, tags, lore.\n\n\
                         Checklist after calling:\n\
                         - [ ] Weave the narrative_hook into the opening scene naturally\n\
                         - [ ] Reference specific items by name when the character uses them\n\
                         - [ ] Use the currency_name for all money references\n\
                         - [ ] Include equipment in the character's inventory state",
                        cfg.binary_path, cfg.genre_packs_path, genre,
                    ),
                    unknown => {
                        warn!(
                            tool = %unknown,
                            "Script tool registered but has no prompt section — narrator will not know how to use it"
                        );
                        continue;
                    }
                };
                builder.add_section(PromptSection::new(
                    &format!("script_tool_{}", tool_name),
                    tool_section,
                    AttentionZone::Valley,
                    SectionCategory::State,
                ));
                script_tools_injected.push(tool_name.clone());
            }
        }

        // Game state section (Valley zone)
        if let Some(state) = &context.state_summary {
            builder.add_section(PromptSection::new(
                "game_state",
                format!("<game_state>\n{}\n</game_state>", state),
                AttentionZone::Valley,
                SectionCategory::State,
            ));
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

        // SFX library (Valley zone)
        if !context.available_sfx.is_empty() {
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

        // Backstory capture directive
        if route.intent() == Intent::Backstory {
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

        // Narrator verbosity instruction (Late zone)
        {
            use sidequest_protocol::NarratorVerbosity;
            let content = match context.narrator_verbosity {
                NarratorVerbosity::Concise => {
                    "[NARRATION LENGTH]\n\
                     Keep descriptions to 1-2 sentences. Prioritize action and \
                     consequence over atmosphere. No extended scene-setting or \
                     sensory elaboration. Be direct."
                }
                NarratorVerbosity::Standard => {
                    "[NARRATION LENGTH]\n\
                     Use standard descriptive prose — balanced detail and pacing. \
                     Include enough atmosphere to set the scene without belaboring it. \
                     2-4 sentences per beat is typical."
                }
                NarratorVerbosity::Verbose => {
                    "[NARRATION LENGTH]\n\
                     Elaborate with sensory details and world-building. Paint the \
                     scene with sights, sounds, smells, and texture. Take time to \
                     establish atmosphere and let moments breathe. 4-6+ sentences \
                     per beat."
                }
            };
            builder.add_section(PromptSection::new(
                "narrator_verbosity",
                content,
                AttentionZone::Late,
                SectionCategory::Format,
            ));
        }

        // Narrator vocabulary instruction (Late zone)
        {
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
            format!("The player says: {}", action),
            AttentionZone::Recency,
            SectionCategory::Action,
        ));

        let section_count = builder.section_count();
        let _pb_guard = tracing::info_span!(
            "turn.agent_llm.prompt_build",
            section_count = section_count as u64,
        ).entered();
        let zone_breakdown = builder.zone_breakdown();
        let prompt_text = builder.compose();
        let allowed_tools = self.narrator_allowed_tools();

        NarratorPromptResult {
            prompt_text,
            zone_breakdown,
            script_tools_injected,
            allowed_tools,
            intent_route: route,
        }
    }

}

impl GameService for Orchestrator {
    fn get_snapshot(&self) -> serde_json::Value {
        serde_json::Value::Object(serde_json::Map::new())
    }

    fn process_action(&self, action: &str, context: &TurnContext) -> ActionResult {
        let span = tracing::info_span!(
            "orchestrator.process_action",
            action_len = action.len(),
            intent = tracing::field::Empty,
            agent = tracing::field::Empty,
            is_degraded = tracing::field::Empty,
        );
        let _guard = span.enter();

        // Build prompt via extracted method (story 15-27: testable prompt assembly).
        let prompt_result = self.build_narrator_prompt(action, context);
        let prompt = prompt_result.prompt_text;
        let prompt_zone_breakdown = prompt_result.zone_breakdown;
        let allowed_tools = prompt_result.allowed_tools;

        // OTEL: report which script tools were injected into this turn's prompt
        if !prompt_result.script_tools_injected.is_empty() {
            let _tools_span = tracing::info_span!(
                "script_tool.prompt_injected",
                tools = %prompt_result.script_tools_injected.join(","),
                count = prompt_result.script_tools_injected.len(),
            )
            .entered();
        }

        // Reuse intent classification from build_narrator_prompt (avoids double Haiku call).
        let route = prompt_result.intent_route;
        span.record("intent", route.intent().to_string().as_str());
        span.record("agent", route.agent_name());
        info!(
            intent = %route.intent(),
            agent = %route.agent_name(),
            source = %route.source(),
            confidence = route.confidence(),
            "Intent classified"
        );

        // Generate a unique session ID for the tool call sidecar file.
        // Tool scripts write results here; we read them after the CLI call completes.
        let sidecar_session_id = format!(
            "turn-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );

        info!(action = %action, sidecar_session_id = %sidecar_session_id, "Invoking Claude CLI for narration");

        let intent_str = route.intent().to_string();
        let agent_str = route.agent_name().to_string();

        // Sonnet for narrator: 3x faster than Opus with acceptable quality.
        // Mechanical consistency enforced by state systems (LoreStore, NPC registry, tropes),
        // not by LLM memory. Structured extraction failures are soft (dropped field, not crash).
        let narrator_model = "sonnet";
        let has_tools = !allowed_tools.is_empty();
        let inference_span = tracing::info_span!(
            "turn.agent_llm.inference",
            model = narrator_model,
            prompt_len = prompt.len(),
            tool_use = has_tools,
        );
        let call_start = std::time::Instant::now();
        let send_result = {
            let _inf_guard = inference_span.enter();
            if has_tools {
                self.client.send_with_tools(&prompt, narrator_model, &allowed_tools)
            } else {
                self.client.send_with_model(&prompt, narrator_model)
            }
        };
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
                // Extract combat patch from creature_smith responses
                let combat_patch = if agent_str == "creature_smith" {
                    match serde_json::from_str::<crate::patches::CombatPatch>(&raw_response)
                        .or_else(|_| extract_fenced_json::<crate::patches::CombatPatch>(&raw_response))
                    {
                        Ok(patch) => {
                            info!(
                                in_combat = ?patch.in_combat,
                                hp_changes = ?patch.hp_changes,
                                drama_weight = ?patch.drama_weight,
                                "combat.patch_extracted"
                            );
                            Some(patch)
                        }
                        Err(e) => {
                            warn!(error = %e, "combat.patch_extraction_failed — creature_smith response had no valid JSON");
                            None
                        }
                    }
                } else {
                    None
                };

                // Extract chase patch from dialectician responses
                let chase_patch = if agent_str == "dialectician" {
                    match serde_json::from_str::<crate::patches::ChasePatch>(&raw_response)
                        .or_else(|_| extract_fenced_json::<crate::patches::ChasePatch>(&raw_response))
                    {
                        Ok(patch) => {
                            info!(
                                in_chase = ?patch.in_chase,
                                separation_delta = ?patch.separation_delta,
                                roll = ?patch.roll,
                                "chase.patch_extracted"
                            );
                            Some(patch)
                        }
                        Err(e) => {
                            warn!(error = %e, "chase.patch_extraction_failed — dialectician response had no valid JSON");
                            None
                        }
                    }
                } else {
                    None
                };

                let agent_duration_ms = call_start.elapsed().as_millis() as u64;
                span.record("is_degraded", false);

                // ADR-057: assemble_turn merges extraction + preprocessor + tool results.
                // Story 20-10: parse sidecar file for tool call results.
                let tool_results = parse_tool_results(&sidecar_session_id);

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
                    combat_patch,
                    chase_patch,
                    is_degraded: false,
                    classified_intent: Some(intent_str),
                    agent_name: Some(agent_str),
                    agent_duration_ms: Some(agent_duration_ms),
                    token_count_in: response.input_tokens.map(|v| v as usize),
                    token_count_out: response.output_tokens.map(|v| v as usize),
                    zone_breakdown: Some(prompt_zone_breakdown),
                    ..base
                }
            }
            Err(e) => {
                let agent_duration_ms = call_start.elapsed().as_millis() as u64;
                panic!(
                    "CLAUDE CLI FAILED — agent={}, duration={}ms, error={}. \
                     If the LLM is down, the game is down.",
                    agent_str, agent_duration_ms, e
                );
            }
        }
    }
}

// ============================================================================
// Combat patch: strip JSON fence from prose after extraction
// ============================================================================

/// Extract and deserialize JSON from a markdown fenced code block.
///
/// Used for creature_smith (CombatPatch) and dialectician (ChasePatch) responses
/// which may wrap their JSON output in ```json ... ``` fences.
fn extract_fenced_json<T: serde::de::DeserializeOwned>(input: &str) -> Result<T, serde_json::Error> {
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

/// Remove a ```json ... ``` fenced block from narration so the player sees clean prose.
fn strip_json_fence(input: &str) -> String {
    let re = regex::Regex::new(r"(?s)```(?:json)?\s*\n[\s\S]*?\n```").unwrap();
    re.replace(input, "").trim().to_string()
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
    #[serde(default)]
    pub you: String,
    #[serde(default)]
    pub named: String,
    #[serde(default)]
    pub intent: String,
}

/// Relevance flags from inline preprocessor (narrator/creature_smith JSON block).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct ActionFlags {
    #[serde(default)]
    pub is_power_grab: bool,
    #[serde(default)]
    pub references_inventory: bool,
    #[serde(default)]
    pub references_npc: bool,
    #[serde(default)]
    pub references_ability: bool,
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
}

/// Extract the narrator's prose from a raw response.
///
/// ADR-057 (story 20-8): The narrator produces pure prose. All structured data
/// (footnotes, items, NPCs, mood, etc.) comes from tool calls collected by
/// `assemble_turn`. This function strips any residual JSON fences and returns
/// the prose with empty structured fields.
fn extract_structured_from_response(raw: &str) -> NarratorExtraction {
    let span = tracing::info_span!("rag.prose_cleanup", raw_len = raw.len());
    let _guard = span.enter();

    let prose = strip_json_fence(raw);

    NarratorExtraction {
        prose,
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: None,
        scene_mood: None,
        personality_events: vec![],
        scene_intent: None,
        resource_deltas: HashMap::new(),
        lore_established: None,
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
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
        .filter(|entry| {
            entry.role.contains("merchant") && entry.location == current_location
        })
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

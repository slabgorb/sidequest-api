//! Orchestrator — state machine that sequences agents and manages game state.
//!
//! Port lesson #1: Server talks to GameService trait, never game internals.
//! ADR-010: Intent-based agent routing via LLM classification.

use std::collections::HashMap;

use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::agent::Agent;
use crate::agents::creature_smith::CreatureSmithAgent;
use crate::agents::dialectician::DialecticianAgent;
use crate::agents::ensemble::EnsembleAgent;
use crate::agents::intent_router::{Intent, IntentRouter};
use crate::agents::narrator::NarratorAgent;
use crate::agents::troper::TroperAgent;
use crate::client::ClaudeClient;
use crate::context_builder::{ContextBuilder, ZoneBreakdown};
use crate::prompt_framework::{parse_soul_md, AttentionZone, PromptSection, SectionCategory};
use crate::turn_record::{TurnIdCounter, TurnRecord};
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
    /// JSON extraction tier used: 1=direct, 2=fenced, 3=regex (for GM Dashboard).
    pub extraction_tier: Option<u8>,
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
    /// SOUL.md principles — injected into every prompt in the Early zone.
    soul_text: Option<String>,
    /// Script tool configurations (ADR-056). Keyed by tool name.
    script_tools: HashMap<String, ScriptToolConfig>,
}

impl Orchestrator {
    /// Create a new orchestrator with a watcher channel sender.
    ///
    /// Automatically loads SOUL.md from the current working directory if present.
    /// SOUL principles are injected into every agent prompt in the Early attention zone.
    pub fn new(watcher_tx: mpsc::Sender<TurnRecord>) -> Self {
        let client = ClaudeClient::new();
        let soul_path = std::path::Path::new("SOUL.md");
        let soul_data = parse_soul_md(soul_path);
        let soul_text = if soul_data.is_empty() {
            info!("SOUL.md not found or empty — prompts will lack guiding principles");
            None
        } else {
            info!(
                principles = soul_data.len(),
                "SOUL.md loaded — {} principles will be injected into every prompt",
                soul_data.len()
            );
            Some(soul_data.as_prompt_text())
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
            soul_text,
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
    fn narrator_allowed_tools(&self) -> Vec<String> {
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

        // ADR-032: Two-tier intent classification (Haiku → narrator fallback)
        let route = self.intent_router.classify(action, context);
        span.record("intent", route.intent().to_string().as_str());
        span.record("agent", route.agent_name());
        info!(
            intent = %route.intent(),
            agent = %route.agent_name(),
            source = %route.source(),
            confidence = route.confidence(),
            "Intent classified"
        );

        // Build prompt via ContextBuilder — zone-ordered, telemetry-instrumented.
        let (prompt, prompt_zone_breakdown) = {
            let mut builder = ContextBuilder::new();

            // Agent identity section (Primacy zone)
            match route.agent_name() {
                "creature_smith" => self.creature_smith.build_context(&mut builder),
                "ensemble" => self.ensemble.build_context(&mut builder),
                "dialectician" => self.dialectician.build_context(&mut builder),
                _ => self.narrator.build_context(&mut builder),
            };

            // SOUL principles (Early zone — high attention, after identity, before state)
            if let Some(ref soul) = self.soul_text {
                builder.add_section(PromptSection::new(
                    "soul_principles",
                    format!("## Guiding Principles\n{}", soul),
                    AttentionZone::Early,
                    SectionCategory::Soul,
                ));
            }

            // Trope beat directives (Early zone — high attention, from previous turn's fired beats)
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
            // Skill-style: clear command reference + checklist for using the result.
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
                }
            }

            // Game state section (Valley zone — lower attention, grounding context)
            if let Some(state) = &context.state_summary {
                builder.add_section(PromptSection::new(
                    "game_state",
                    format!("<game_state>\n{}\n</game_state>", state),
                    AttentionZone::Valley,
                    SectionCategory::State,
                ));
            }

            // Active trope summary (Valley zone — background context for all agents)
            if let Some(ref trope_summary) = context.active_trope_summary {
                builder.add_section(PromptSection::new(
                    "active_tropes",
                    trope_summary.clone(),
                    AttentionZone::Valley,
                    SectionCategory::State,
                ));
            }

            // Backstory capture directive — when the player is building their character's
            // history, tell the narrator to extract personal details as footnotes so they
            // persist in the RAG knowledge store.
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

            // Narrator verbosity instruction (Late zone — high recency attention for format guidance)
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

            // Narrator vocabulary instruction (Late zone — high recency attention for format guidance)
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

            // Player action section (Recency zone — highest attention at prompt end)
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
            // Capture zone breakdown before composing (story 18-6)
            let zb = builder.zone_breakdown();
            let prompt_text = builder.compose();
            (prompt_text, zb)
        };

        info!(action = %action, "Invoking Claude CLI for narration");

        let intent_str = route.intent().to_string();
        let agent_str = route.agent_name().to_string();

        // Sonnet for narrator: 3x faster than Opus with acceptable quality.
        // Mechanical consistency enforced by state systems (LoreStore, NPC registry, tropes),
        // not by LLM memory. Structured extraction failures are soft (dropped field, not crash).
        let narrator_model = "sonnet";
        let allowed_tools = self.narrator_allowed_tools();
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
                // Extract structured data from narrator response (footnotes + items)
                let extraction_span = tracing::info_span!(
                    "turn.agent_llm.extraction",
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
                    match crate::extractor::JsonExtractor::extract::<crate::patches::CombatPatch>(
                        &raw_response,
                    ) {
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
                            warn!(error = %e, "combat.patch_extraction_failed — creature_smith response had no valid JSON block");
                            None
                        }
                    }
                } else {
                    None
                };

                // Extract chase patch from dialectician responses
                let chase_patch = if agent_str == "dialectician" {
                    match crate::extractor::JsonExtractor::extract::<crate::patches::ChasePatch>(
                        &raw_response,
                    ) {
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
                            warn!(error = %e, "chase.patch_extraction_failed — dialectician response had no valid JSON block");
                            None
                        }
                    }
                } else {
                    None
                };

                // Strip the JSON fence block from narration so prose is clean
                let narration = if combat_patch.is_some() || chase_patch.is_some() {
                    strip_json_fence(&extraction.prose)
                } else {
                    extraction.prose
                };

                let agent_duration_ms = call_start.elapsed().as_millis() as u64;
                info!(
                    len = narration.len(),
                    duration_ms = agent_duration_ms,
                    "Claude CLI returned narration"
                );
                span.record("is_degraded", false);
                ActionResult {
                    narration,
                    combat_patch,
                    chase_patch,
                    is_degraded: false,
                    classified_intent: Some(intent_str),
                    agent_name: Some(agent_str),
                    footnotes: extraction.footnotes,
                    items_gained: extraction.items_gained,
                    npcs_present: extraction.npcs_present,
                    quest_updates: extraction.quest_updates,
                    agent_duration_ms: Some(agent_duration_ms),
                    token_count_in: response.input_tokens.map(|v| v as usize),
                    token_count_out: response.output_tokens.map(|v| v as usize),
                    extraction_tier: Some(extraction.tier),
                    visual_scene: extraction.visual_scene,
                    scene_mood: extraction.scene_mood,
                    personality_events: extraction.personality_events,
                    scene_intent: extraction.scene_intent,
                    resource_deltas: extraction.resource_deltas,
                    zone_breakdown: Some(prompt_zone_breakdown),
                    lore_established: extraction.lore_established,
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

/// Contains footnotes, items gained, NPCs present, and quest updates in the narrator's response.
#[derive(Debug, serde::Deserialize)]
struct NarratorStructuredBlock {
    #[serde(default)]
    footnotes: Vec<sidequest_protocol::Footnote>,
    #[serde(default)]
    items_gained: Vec<sidequest_protocol::ItemGained>,
    #[serde(default)]
    npcs_present: Vec<NpcMention>,
    #[serde(default)]
    quest_updates: HashMap<String, String>,
    #[serde(default)]
    visual_scene: Option<VisualScene>,
    #[serde(default)]
    scene_mood: Option<String>,
    #[serde(default)]
    personality_events: Vec<PersonalityEvent>,
    #[serde(default)]
    scene_intent: Option<String>,
    #[serde(default)]
    resource_deltas: HashMap<String, f64>,
    #[serde(default)]
    lore_established: Option<Vec<String>>,
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
    /// Extraction tier: 1=fenced JSON, 2=legacy array, 3=no structured data.
    pub tier: u8,
}

/// Extract structured data (footnotes, items) from a narrator response.
///
/// The narrator embeds a JSON block after the prose containing footnotes
/// and items_gained. This function finds and parses that block, returning
/// the clean prose and extracted structured data.
fn extract_structured_from_response(raw: &str) -> NarratorExtraction {
    let span = tracing::info_span!("rag.extract_structured", raw_len = raw.len());
    let _guard = span.enter();

    // Strategy 1: Fenced JSON block (```json ... ```)
    if let Some(start) = raw.find("```json") {
        if let Some(end) = raw[start + 7..].find("```") {
            let json_str = raw[start + 7..start + 7 + end].trim();
            if let Ok(block) = serde_json::from_str::<NarratorStructuredBlock>(json_str) {
                let prose = raw[..start].trim().to_string();
                tracing::info!(
                    footnotes = block.footnotes.len(),
                    items = block.items_gained.len(),
                    strategy = "fenced_json",
                    "rag.structured_parsed"
                );
                return NarratorExtraction {
                    prose,
                    footnotes: block.footnotes,
                    items_gained: block.items_gained,
                    npcs_present: block.npcs_present,
                    quest_updates: block.quest_updates,
                    visual_scene: block.visual_scene,
                    scene_mood: block.scene_mood,
                    personality_events: block.personality_events,
                    scene_intent: block.scene_intent,
                    resource_deltas: block.resource_deltas,
                    lore_established: block.lore_established,
                    tier: 1,
                };
            }
            // Try parsing as a bare footnotes array (legacy format)
            if let Ok(footnotes) =
                serde_json::from_str::<Vec<sidequest_protocol::Footnote>>(json_str)
            {
                let prose = raw[..start].trim().to_string();
                tracing::info!(
                    footnotes = footnotes.len(),
                    strategy = "fenced_array",
                    "rag.structured_parsed"
                );
                return NarratorExtraction {
                    prose,
                    footnotes,
                    items_gained: vec![],
                    npcs_present: vec![],
                    quest_updates: HashMap::new(),
                    visual_scene: None,
                    scene_mood: None,
                    personality_events: vec![],
                    scene_intent: None,
                    resource_deltas: HashMap::new(),
                    lore_established: None,
                    tier: 2,
                };
            }
        }
    }

    // Strategy 2: Trailing JSON object
    if let Some(idx) = raw.rfind("{\"footnotes\"") {
        let json_str = &raw[idx..];
        if let Ok(block) = serde_json::from_str::<NarratorStructuredBlock>(json_str) {
            let prose = raw[..idx].trim().to_string();
            tracing::info!(
                footnotes = block.footnotes.len(),
                items = block.items_gained.len(),
                strategy = "trailing_json",
                "rag.structured_parsed"
            );
            return NarratorExtraction {
                prose,
                footnotes: block.footnotes,
                items_gained: block.items_gained,
                npcs_present: block.npcs_present,
                quest_updates: block.quest_updates,
                visual_scene: block.visual_scene,
                scene_mood: block.scene_mood,
                personality_events: block.personality_events,
                scene_intent: block.scene_intent,
                resource_deltas: block.resource_deltas,
                lore_established: block.lore_established,
                tier: 2,
            };
        }
    }

    // Also try items_gained as the leading key
    if let Some(idx) = raw.rfind("{\"items_gained\"") {
        let json_str = &raw[idx..];
        if let Ok(block) = serde_json::from_str::<NarratorStructuredBlock>(json_str) {
            let prose = raw[..idx].trim().to_string();
            return NarratorExtraction {
                prose,
                footnotes: block.footnotes,
                items_gained: block.items_gained,
                npcs_present: block.npcs_present,
                quest_updates: block.quest_updates,
                visual_scene: block.visual_scene,
                scene_mood: block.scene_mood,
                personality_events: block.personality_events,
                scene_intent: block.scene_intent,
                resource_deltas: block.resource_deltas,
                lore_established: block.lore_established,
                tier: 2,
            };
        }
    }

    // No structured data found
    tracing::debug!("rag.no_structured_data_found");
    NarratorExtraction {
        prose: raw.to_string(),
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
        tier: 3,
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

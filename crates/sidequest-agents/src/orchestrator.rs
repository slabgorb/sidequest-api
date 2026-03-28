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
use crate::agents::intent_router::IntentRouter;
#[allow(unused_imports)] // classify_with_state is a static method
use crate::agents::narrator::NarratorAgent;
use crate::client::ClaudeClient;
use crate::context_builder::ContextBuilder;
use crate::prompt_framework::{AttentionZone, PromptSection, SectionCategory};
use crate::turn_record::{TurnIdCounter, TurnRecord};
use sidequest_game::tension_tracker::{DeliveryMode, DramaThresholds, TensionTracker};

/// Result of processing a player action through the orchestrator.
#[derive(Debug, Clone)]
pub struct ActionResult {
    /// The narrative text produced by the agent.
    pub narration: String,
    /// Optional state delta for the client.
    pub state_delta: Option<HashMap<String, serde_json::Value>>,
    /// Combat events that occurred during this action.
    pub combat_events: Vec<String>,
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
    /// Specialist agents — dispatched by intent classification.
    narrator: NarratorAgent,
    creature_smith: CreatureSmithAgent,
    ensemble: EnsembleAgent,
    dialectician: DialecticianAgent,
    /// Pacing engine — tracks drama weight across combat turns (Story 5-7).
    tension_tracker: TensionTracker,
    /// Genre-tunable pacing breakpoints (Story 5-7).
    drama_thresholds: DramaThresholds,
}

impl Orchestrator {
    /// Create a new orchestrator with a watcher channel sender.
    pub fn new(watcher_tx: mpsc::Sender<TurnRecord>) -> Self {
        let client = ClaudeClient::new();
        Self {
            watcher_tx,
            turn_id_counter: TurnIdCounter::new(),
            client,
            narrator: NarratorAgent::new(),
            creature_smith: CreatureSmithAgent::new(),
            ensemble: EnsembleAgent::new(),
            dialectician: DialecticianAgent::new(),
            tension_tracker: TensionTracker::new(),
            drama_thresholds: DramaThresholds::default(),
        }
    }

    /// Access the tension tracker (pacing engine).
    pub fn tension(&self) -> &TensionTracker {
        &self.tension_tracker
    }

    /// Access the drama thresholds (genre-tunable pacing breakpoints).
    pub fn drama_thresholds(&self) -> &DramaThresholds {
        &self.drama_thresholds
    }
}

impl GameService for Orchestrator {
    fn get_snapshot(&self) -> serde_json::Value {
        serde_json::Value::Object(serde_json::Map::new())
    }

    fn process_action(&self, action: &str, context: &TurnContext) -> ActionResult {
        // ADR-032: Two-tier intent classification (state override → keyword fallback)
        let route = IntentRouter::classify_with_state(action, context);
        info!(
            intent = %route.intent(),
            agent = %route.agent_name(),
            "Intent classified"
        );

        // Build prompt via ContextBuilder — zone-ordered, telemetry-instrumented.
        let mut builder = ContextBuilder::new();

        // Agent identity section (Primacy zone)
        match route.agent_name() {
            "creature_smith" => self.creature_smith.build_context(&mut builder),
            "ensemble" => self.ensemble.build_context(&mut builder),
            "dialectician" => self.dialectician.build_context(&mut builder),
            _ => self.narrator.build_context(&mut builder),
        };

        // Game state section (Valley zone — lower attention, grounding context)
        if let Some(state) = &context.state_summary {
            builder.add_section(PromptSection::new(
                "game_state",
                format!("<game_state>\n{}\n</game_state>", state),
                AttentionZone::Valley,
                SectionCategory::State,
            ));
        }

        // Player action section (Recency zone — highest attention at prompt end)
        builder.add_section(PromptSection::new(
            "player_action",
            format!("The player says: {}", action),
            AttentionZone::Recency,
            SectionCategory::Action,
        ));

        let prompt = builder.compose();

        info!(action = %action, "Invoking Claude CLI for narration");

        let intent_str = route.intent().to_string();
        let agent_str = route.agent_name().to_string();

        match self.client.send(&prompt) {
            Ok(raw_response) => {
                // Extract structured data from narrator response (footnotes + items)
                let extraction = extract_structured_from_response(&raw_response);
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
                info!(len = extraction.prose.len(), "Claude CLI returned narration");
                ActionResult {
                    narration: extraction.prose,
                    state_delta: Some(HashMap::new()),
                    combat_events: vec![],
                    is_degraded: false,
                    classified_intent: Some(intent_str),
                    agent_name: Some(agent_str),
                    footnotes: extraction.footnotes,
                    items_gained: extraction.items_gained,
                }
            }
            Err(e) => {
                warn!(error = %e, action = %action, "Claude CLI failed, returning degraded response");
                ActionResult {
                    narration: format!(
                        "The world shimmers uncertainly... (narrator unavailable: {})",
                        e
                    ),
                    state_delta: Some(HashMap::new()),
                    combat_events: vec![],
                    is_degraded: true,
                    classified_intent: Some(intent_str),
                    agent_name: Some(agent_str),
                    footnotes: vec![],
                    items_gained: vec![],
                }
            }
        }
    }
}

// ============================================================================
// Story 9-11: Footnote extraction from narrator response
// ============================================================================

/// Serde model for the narrator's structured JSON output block.
/// Contains footnotes and optionally items gained by the player.
#[derive(Debug, serde::Deserialize)]
struct NarratorStructuredBlock {
    #[serde(default)]
    footnotes: Vec<sidequest_protocol::Footnote>,
    #[serde(default)]
    items_gained: Vec<sidequest_protocol::ItemGained>,
}

/// Extracted structured data from a narrator response.
struct NarratorExtraction {
    prose: String,
    footnotes: Vec<sidequest_protocol::Footnote>,
    items_gained: Vec<sidequest_protocol::ItemGained>,
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
                return NarratorExtraction { prose, footnotes: block.footnotes, items_gained: block.items_gained };
            }
            // Try parsing as a bare footnotes array (legacy format)
            if let Ok(footnotes) = serde_json::from_str::<Vec<sidequest_protocol::Footnote>>(json_str) {
                let prose = raw[..start].trim().to_string();
                tracing::info!(
                    footnotes = footnotes.len(),
                    strategy = "fenced_array",
                    "rag.structured_parsed"
                );
                return NarratorExtraction { prose, footnotes, items_gained: vec![] };
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
            return NarratorExtraction { prose, footnotes: block.footnotes, items_gained: block.items_gained };
        }
    }

    // Also try items_gained as the leading key
    if let Some(idx) = raw.rfind("{\"items_gained\"") {
        let json_str = &raw[idx..];
        if let Ok(block) = serde_json::from_str::<NarratorStructuredBlock>(json_str) {
            let prose = raw[..idx].trim().to_string();
            return NarratorExtraction { prose, footnotes: block.footnotes, items_gained: block.items_gained };
        }
    }

    // No structured data found
    tracing::debug!("rag.no_structured_data_found");
    NarratorExtraction { prose: raw.to_string(), footnotes: vec![], items_gained: vec![] }
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

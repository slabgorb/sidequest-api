//! Intent router — LLM-based classification of player input to agent.
//!
//! ADR-010: Intent-based agent routing. An LLM classifier routes each player
//! input to a specialist agent based on intent and current game state.
//!
//! ADR-032: Haiku classifier with narrator ambiguity resolution.
//! When Haiku is unavailable, the narrator handles intent resolution
//! directly — no keyword fallback.

use crate::client::ClaudeClient;
use crate::context_builder::ContextBuilder;
use crate::prompt_framework::{AttentionZone, PromptSection, SectionCategory};

/// Player intent categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Intent {
    /// Combat actions (attack, defend, use ability).
    Combat,
    /// Dialogue with NPCs.
    Dialogue,
    /// Exploration and movement.
    Exploration,
    /// Examining objects or the environment.
    Examine,
    /// Meta commands (save, help, status).
    Meta,
    /// Chase sequences (pursuit, escape, negotiation while fleeing).
    Chase,
}

impl Intent {
    /// Whether this intent represents a meaningful player action that resets
    /// the engagement counter. Combat, Dialogue, and Chase are meaningful
    /// (the player is actively driving the story). Exploration, Examine, and
    /// Meta are not (idle browsing or system commands).
    pub fn is_meaningful(&self) -> bool {
        matches!(self, Intent::Combat | Intent::Dialogue | Intent::Chase)
    }
}

impl std::fmt::Display for Intent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Intent::Combat => write!(f, "Combat"),
            Intent::Dialogue => write!(f, "Dialogue"),
            Intent::Exploration => write!(f, "Exploration"),
            Intent::Examine => write!(f, "Examine"),
            Intent::Meta => write!(f, "Meta"),
            Intent::Chase => write!(f, "Chase"),
        }
    }
}

/// How the intent was determined (ADR-032).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClassificationSource {
    /// Haiku LLM classifier — normal path.
    Haiku,
    /// State override — fast path (in_combat, in_chase).
    StateOverride,
    /// Haiku was unavailable — narrator will resolve intent directly.
    HaikuUnavailable,
}

impl std::fmt::Display for ClassificationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClassificationSource::Haiku => write!(f, "Haiku"),
            ClassificationSource::StateOverride => write!(f, "StateOverride"),
            ClassificationSource::HaikuUnavailable => write!(f, "HaikuUnavailable"),
        }
    }
}

/// A routing decision mapping an intent to an agent, with confidence scoring (ADR-032).
#[derive(Debug, Clone)]
pub struct IntentRoute {
    agent_name: String,
    intent: Intent,
    confidence: f64,
    candidates: Vec<Intent>,
    source: ClassificationSource,
}

impl IntentRoute {
    /// Map an intent to its specialist agent name.
    fn agent_for(intent: Intent) -> &'static str {
        match intent {
            Intent::Combat => "creature_smith",
            Intent::Dialogue => "ensemble",
            Intent::Exploration => "narrator",
            Intent::Examine => "narrator",
            Intent::Meta => "narrator",
            Intent::Chase => "dialectician",
        }
    }

    /// Create a route for a given intent with full confidence.
    pub fn for_intent(intent: Intent) -> Self {
        Self {
            agent_name: Self::agent_for(intent).to_string(),
            intent,
            confidence: 1.0,
            candidates: vec![],
            source: ClassificationSource::Haiku,
        }
    }

    /// Narrator fallback — Haiku is down, let the narrator sort it out.
    pub fn narrator_fallback() -> Self {
        Self {
            agent_name: "narrator".to_string(),
            intent: Intent::Exploration,
            confidence: 0.0,
            candidates: vec![],
            source: ClassificationSource::HaikuUnavailable,
        }
    }

    /// Validated constructor — returns Err if confidence is outside 0.0..=1.0.
    pub fn try_with_classification(
        intent: Intent,
        confidence: f64,
        candidates: Vec<Intent>,
        source: ClassificationSource,
    ) -> Result<Self, String> {
        if !(0.0..=1.0).contains(&confidence) {
            return Err(format!(
                "confidence must be 0.0..=1.0, got {confidence}"
            ));
        }
        Ok(Self {
            agent_name: Self::agent_for(intent).to_string(),
            intent,
            confidence,
            candidates,
            source,
        })
    }

    /// Create a route with full classification data (ADR-032).
    ///
    /// Panics if confidence is outside 0.0..=1.0. Use `try_with_classification`
    /// at trust boundaries.
    pub fn with_classification(
        intent: Intent,
        confidence: f64,
        candidates: Vec<Intent>,
        source: ClassificationSource,
    ) -> Self {
        Self::try_with_classification(intent, confidence, candidates, source)
            .expect("confidence must be 0.0..=1.0")
    }

    /// The agent name this route points to.
    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

    /// The classified intent.
    pub fn intent(&self) -> Intent {
        self.intent
    }

    /// Classification confidence (0.0-1.0).
    pub fn confidence(&self) -> f64 {
        self.confidence
    }

    /// Alternative intent candidates when classification is ambiguous.
    pub fn candidates(&self) -> &[Intent] {
        &self.candidates
    }

    /// How the classification was determined.
    pub fn source(&self) -> ClassificationSource {
        self.source
    }

    /// Whether this classification is ambiguous (confidence < 0.5 from Haiku).
    pub fn is_ambiguous(&self) -> bool {
        self.source == ClassificationSource::Haiku && self.confidence < 0.5
    }

    /// Whether this route's intent is a meaningful player action.
    /// Delegates to [`Intent::is_meaningful()`].
    pub fn is_meaningful(&self) -> bool {
        self.intent.is_meaningful()
    }
}

/// Trait for intent classifiers (ADR-032).
///
/// Implementations include the Haiku LLM classifier and test mocks.
pub trait IntentClassifier {
    /// Classify player input given the current turn context.
    fn classify(&self, input: &str, context: &crate::orchestrator::TurnContext) -> IntentRoute;
}

/// Haiku LLM classifier — calls `claude -p --model haiku` to classify player actions (ADR-032).
pub struct HaikuClassifier {
    client: ClaudeClient,
}

impl HaikuClassifier {
    /// Create a new Haiku classifier with the given Claude client.
    pub fn new(client: ClaudeClient) -> Self {
        Self { client }
    }

    /// Build the classification prompt per ADR-032.
    fn build_prompt(input: &str, context: &crate::orchestrator::TurnContext) -> String {
        let state_context = context
            .state_summary
            .as_deref()
            .unwrap_or("No scene context available.");

        format!(
            "You classify player actions in a tabletop RPG.\n\
             Given the player's action and current scene context, return a JSON object:\n\
             {{ \"intent\": \"<Combat|Dialogue|Exploration|Examine|Meta|Chase>\",\n\
               \"confidence\": <0.0-1.0>,\n\
               \"candidates\": [\"<intent>\", ...] }}\n\n\
             If the action clearly maps to one intent, return confidence >= 0.8.\n\
             If the action is ambiguous (could be multiple intents), return\n\
               intent set to your best guess, confidence < 0.5, and list the top candidates.\n\n\
             Scene context: {state_context}\n\n\
             Player action: {input}\n\n\
             Return ONLY the JSON object, no other text."
        )
    }

    /// Strip markdown code fences that LLMs sometimes wrap around JSON.
    ///
    /// Handles ```json\n{...}\n``` and ```\n{...}\n``` variants.
    fn strip_code_fences(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.starts_with("```") {
            // Remove opening fence (```json or ```)
            let after_open = if let Some(pos) = trimmed.find('\n') {
                &trimmed[pos + 1..]
            } else {
                return trimmed.to_string();
            };
            // Remove closing fence
            let content = after_open.trim_end();
            let content = content.strip_suffix("```").unwrap_or(content);
            content.trim().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Parse the JSON response from Haiku into an IntentRoute.
    fn parse_response(raw: &str) -> Option<IntentRoute> {
        let cleaned = Self::strip_code_fences(raw);
        let value: serde_json::Value = serde_json::from_str(&cleaned).ok()?;

        let intent_str = value.get("intent")?.as_str()?;
        let intent = match intent_str {
            "Combat" => Intent::Combat,
            "Dialogue" => Intent::Dialogue,
            "Exploration" => Intent::Exploration,
            "Examine" => Intent::Examine,
            "Meta" => Intent::Meta,
            "Chase" => Intent::Chase,
            _ => return None,
        };

        let confidence = value.get("confidence")?.as_f64()?;
        let confidence = confidence.clamp(0.0, 1.0);

        let candidates: Vec<Intent> = value
            .get("candidates")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(|s| match s {
                        "Combat" => Some(Intent::Combat),
                        "Dialogue" => Some(Intent::Dialogue),
                        "Exploration" => Some(Intent::Exploration),
                        "Examine" => Some(Intent::Examine),
                        "Meta" => Some(Intent::Meta),
                        "Chase" => Some(Intent::Chase),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        IntentRoute::try_with_classification(intent, confidence, candidates, ClassificationSource::Haiku).ok()
    }
}

impl IntentClassifier for HaikuClassifier {
    fn classify(&self, input: &str, context: &crate::orchestrator::TurnContext) -> IntentRoute {
        let prompt = Self::build_prompt(input, context);

        match self.client.send_with_model(&prompt, "haiku") {
            Ok(raw) => {
                match Self::parse_response(&raw) {
                    Some(route) => route,
                    None => {
                        tracing::warn!(
                            raw_response = %raw,
                            "Haiku returned unparseable response, routing to narrator"
                        );
                        IntentRoute::narrator_fallback()
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Haiku classifier unavailable, routing to narrator"
                );
                IntentRoute::narrator_fallback()
            }
        }
    }
}

/// Routes player input to the appropriate agent via LLM classification.
pub struct IntentRouter {
    classifier: HaikuClassifier,
}

impl IntentRouter {
    /// Create a new intent router with a Claude client.
    pub fn new(client: ClaudeClient) -> Self {
        let classifier = HaikuClassifier::new(client);
        Self { classifier }
    }

    /// Classify using the Haiku classifier wired into this router.
    ///
    /// This is the production entry point — called from the orchestrator's turn loop.
    pub fn classify(&self, input: &str, ctx: &crate::orchestrator::TurnContext) -> IntentRoute {
        Self::classify_with_classifier(input, ctx, &self.classifier)
    }

    /// Classification pipeline (ADR-032).
    ///
    /// 1. State override (in_combat/in_chase) → immediate dispatch
    /// 2. Haiku classifier → dispatch on result; narrator handles failures
    pub fn classify_with_classifier(
        input: &str,
        ctx: &crate::orchestrator::TurnContext,
        classifier: &dyn IntentClassifier,
    ) -> IntentRoute {
        // Fast path: state overrides bypass classification
        if ctx.in_chase {
            let route = IntentRoute::with_classification(
                Intent::Chase,
                1.0,
                vec![],
                ClassificationSource::StateOverride,
            );
            Self::emit_span(input, &route);
            return route;
        }
        if ctx.in_combat {
            let route = IntentRoute::with_classification(
                Intent::Combat,
                1.0,
                vec![],
                ClassificationSource::StateOverride,
            );
            Self::emit_span(input, &route);
            return route;
        }

        // Haiku classifier — narrator handles failures
        let route = classifier.classify(input, ctx);
        Self::emit_span(input, &route);
        route
    }

    /// Emit a tracing span for classification.
    fn emit_span(input: &str, route: &IntentRoute) {
        let intent_str = format!("{:?}", route.intent());
        let _span = tracing::info_span!(
            "classify_intent",
            player_input = %input,
            classified_intent = %intent_str,
            agent_routed_to = %route.agent_name(),
            source = %route.source(),
            confidence = route.confidence(),
            is_ambiguous = route.is_ambiguous(),
        )
        .entered();
    }

    /// Add ambiguity context to the narrator prompt when classification is ambiguous (ADR-032).
    ///
    /// When Haiku returns low confidence, the candidates are folded into the narrator's
    /// prompt so it can resolve the ambiguity with full scene context.
    pub fn add_ambiguity_context(builder: &mut ContextBuilder, route: &IntentRoute) {
        if !route.is_ambiguous() || route.candidates().is_empty() {
            return;
        }

        let candidates_str: Vec<String> = route
            .candidates()
            .iter()
            .map(|c| format!("{c}"))
            .collect();
        let candidates_list = candidates_str.join(", ");

        let content = format!(
            "Intent classification was ambiguous between {candidates_list}. \
             Based on the current scene context, use your judgment to determine \
             which specialist behavior to adopt for this narration."
        );

        builder.add_section(PromptSection::new(
            "intent_ambiguity",
            content,
            AttentionZone::Late,
            SectionCategory::Context,
        ));
    }
}

//! Intent router — state-based inference of player intent (ADR-067).
//!
//! ADR-067: Unified narrator agent. Intent classification no longer requires
//! an LLM call. Combat and chase are inferred from game state; everything
//! else goes to the narrator. The Intent enum and IntentRoute struct are
//! retained for OTEL telemetry and conditional prompt section injection.

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
    /// Backstory — player is describing their character's history, personality,
    /// possessions, or identity. Should be captured as character-keyed KnownFacts.
    Backstory,
    /// Accusation — player accuses an NPC in a scenario (Epic 7).
    Accusation,
}

impl Intent {
    /// Whether this intent represents a meaningful player action that resets
    /// the engagement counter. Combat, Dialogue, and Chase are meaningful
    /// (the player is actively driving the story). Exploration, Examine, and
    /// Meta are not (idle browsing or system commands).
    pub fn is_meaningful(&self) -> bool {
        matches!(self, Intent::Combat | Intent::Dialogue | Intent::Chase | Intent::Backstory | Intent::Accusation)
    }
}

impl std::fmt::Display for Intent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Intent::Combat => write!(f, "Combat"),
            Intent::Dialogue => write!(f, "Dialogue"),
            Intent::Exploration => write!(f, "Exploration"),
            Intent::Backstory => write!(f, "Backstory"),
            Intent::Examine => write!(f, "Examine"),
            Intent::Meta => write!(f, "Meta"),
            Intent::Chase => write!(f, "Chase"),
            Intent::Accusation => write!(f, "Accusation"),
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
    /// Map an intent to its agent name (ADR-067: unified narrator).
    /// All intents route to the narrator. Combat/chase/dialogue rules
    /// are injected as conditional prompt sections.
    fn agent_for(_intent: Intent) -> &'static str {
        "narrator"
    }

    /// Create a route for a given intent with full confidence.
    #[doc(hidden)]
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

/// Routes player input to the narrator via state-based inference (ADR-067).
///
/// No LLM call. Combat and chase are detected from game state flags.
/// Everything else is Exploration — the narrator handles all intents.
pub struct IntentRouter;

impl IntentRouter {
    /// Create a new intent router (ADR-067: no Claude client needed).
    pub fn new(_client: crate::client::ClaudeClient) -> Self {
        Self
    }

    /// State-based intent inference (ADR-067).
    ///
    /// No LLM call. Combat/chase from state flags, everything else Exploration.
    pub fn classify(&self, input: &str, ctx: &crate::orchestrator::TurnContext) -> IntentRoute {
        Self::classify_with_classifier(input, ctx, &NoOpClassifier)
    }

    /// Classification pipeline (ADR-067).
    ///
    /// 1. State override (in_chase/in_combat) -> immediate dispatch
    /// 2. Default to Exploration (narrator handles everything)
    pub fn classify_with_classifier(
        input: &str,
        ctx: &crate::orchestrator::TurnContext,
        _classifier: &dyn IntentClassifier,
    ) -> IntentRoute {
        // Fast path: state overrides
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

        // Default: Exploration — narrator handles everything (ADR-067)
        let route = IntentRoute::with_classification(
            Intent::Exploration,
            1.0,
            vec![],
            ClassificationSource::StateOverride,
        );
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

    /// Add ambiguity context to the narrator prompt when classification is ambiguous.
    /// ADR-067: With state-based inference, ambiguity no longer occurs, but this
    /// method is retained for API compatibility.
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

/// No-op classifier used internally — state overrides handle all classification (ADR-067).
struct NoOpClassifier;

impl IntentClassifier for NoOpClassifier {
    fn classify(&self, _input: &str, _context: &crate::orchestrator::TurnContext) -> IntentRoute {
        IntentRoute::for_intent(Intent::Exploration)
    }
}

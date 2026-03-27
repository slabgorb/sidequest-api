//! Intent router — LLM-based classification of player input to agent.
//!
//! ADR-010: Intent-based agent routing. An LLM classifier routes each player
//! input to a specialist agent based on intent and current game state.
//!
//! ADR-032: Two-tier intent classification. Haiku classifier with narrator
//! ambiguity resolution replaces keyword substring matching.

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
    /// Keyword fallback — degraded mode when Haiku is unavailable.
    KeywordFallback,
}

impl std::fmt::Display for ClassificationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClassificationSource::Haiku => write!(f, "Haiku"),
            ClassificationSource::StateOverride => write!(f, "StateOverride"),
            ClassificationSource::KeywordFallback => write!(f, "KeywordFallback"),
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
    /// Create a route for a given intent (keyword fallback path).
    /// Confidence is 1.0 for direct matches, used by existing callers.
    pub fn for_intent(intent: Intent) -> Self {
        let agent_name = match intent {
            Intent::Combat => "creature_smith",
            Intent::Dialogue => "ensemble",
            Intent::Exploration => "narrator",
            Intent::Examine => "narrator",
            Intent::Meta => "narrator",
            Intent::Chase => "dialectician",
        };
        Self {
            agent_name: agent_name.to_string(),
            intent,
            confidence: 1.0,
            candidates: vec![],
            source: ClassificationSource::KeywordFallback,
        }
    }

    /// Fallback route — defaults to Narrator with lower confidence (ADR-010).
    pub fn fallback() -> Self {
        Self {
            agent_name: "narrator".to_string(),
            intent: Intent::Exploration,
            confidence: 0.5,
            candidates: vec![],
            source: ClassificationSource::KeywordFallback,
        }
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
        assert!(
            (0.0..=1.0).contains(&confidence),
            "confidence must be 0.0..=1.0, got {confidence}"
        );
        let agent_name = match intent {
            Intent::Combat => "creature_smith",
            Intent::Dialogue => "ensemble",
            Intent::Exploration => "narrator",
            Intent::Examine => "narrator",
            Intent::Meta => "narrator",
            Intent::Chase => "dialectician",
        };
        Self {
            agent_name: agent_name.to_string(),
            intent,
            confidence,
            candidates,
            source,
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
        Ok(Self::with_classification(intent, confidence, candidates, source))
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
}

/// Trait for intent classifiers (ADR-032).
///
/// Implementations include the Haiku LLM classifier and test mocks.
pub trait IntentClassifier {
    /// Classify player input given the current turn context.
    fn classify(&self, input: &str, context: &crate::orchestrator::TurnContext) -> IntentRoute;
}

/// Routes player input to the appropriate agent via LLM classification.
pub struct IntentRouter {
    #[allow(dead_code)]
    client: ClaudeClient,
}

impl IntentRouter {
    /// Create a new intent router with a Claude client.
    pub fn new(client: ClaudeClient) -> Self {
        Self { client }
    }

    /// Classify player input using keyword matching only (no LLM call).
    ///
    /// This is the synchronous fast path / degraded fallback (ADR-032).
    /// Emits a tracing span with semantic fields for agent telemetry (story 3-1).
    pub fn classify_keywords(input: &str) -> IntentRoute {
        let route = Self::classify_keywords_inner(input);
        let is_fallback = route.agent_name() == "narrator"
            && route.intent() == Intent::Exploration
            && !Self::has_word(&input.to_lowercase(), "look")
            && !Self::has_word(&input.to_lowercase(), "go")
            && !Self::has_word(&input.to_lowercase(), "explore");

        let intent_str = format!("{:?}", route.intent());
        let span = tracing::info_span!(
            "classify_keywords",
            player_input = %input,
            classified_intent = %intent_str,
            agent_routed_to = %route.agent_name(),
            confidence = route.confidence(),
            fallback_used = is_fallback,
            source = %route.source(),
        );
        let _guard = span.enter();

        // Also record via deferred pattern for telemetry consumers that
        // observe Span::record() events (story 3-1 AC: deferred fields).
        span.record("classified_intent", &tracing::field::display(&intent_str));
        span.record("agent_routed_to", &route.agent_name());

        route
    }

    /// Check if a word appears as a whole word in text (not as a substring).
    fn has_word(text: &str, word: &str) -> bool {
        // For multi-word phrases, use contains (they're specific enough)
        if word.contains(' ') {
            return text.contains(word);
        }
        // For single words, check word boundaries
        for candidate in text.split(|c: char| !c.is_alphanumeric() && c != '\'') {
            if candidate == word {
                return true;
            }
        }
        false
    }

    /// Inner keyword classification logic (no tracing).
    fn classify_keywords_inner(input: &str) -> IntentRoute {
        let lower = input.to_lowercase();

        // Combat keywords — physical violence, weapon use, ability activation
        let combat_words = [
            "attack",
            "slash",
            "strike",
            "cast",
            "shoot",
            "defend",
            "stab",
            "fight",
            "hit",
            "swing",
            "parry",
            "block",
            "spell",
            "lunge",
            "grab",
            "throw",
            "punch",
            "kick",
            "shove",
            "disarm",
            "dodge",
            "wrestle",
            "draw my sword",
            "draw my weapon",
            "charge",
            "tackle",
            "bite",
            "claw",
            "smash",
            "bash",
            "cleave",
            "fire at",
            "aim",
            "snipe",
            "ambush",
            "grapple",
            "choke",
            "headbutt",
        ];
        if combat_words.iter().any(|w| Self::has_word(&lower, w)) {
            return IntentRoute::for_intent(Intent::Combat);
        }

        // Dialogue keywords — conversation, persuasion, social manipulation
        let dialogue_words = [
            "talk",
            "tell",
            "ask",
            "say",
            "speak",
            "greet",
            "persuade",
            "negotiate",
            "threaten",
            "lie",
            "bluff",
            "convince",
            "bribe",
            "intimidate",
            "charm",
            "flatter",
            "demand",
            "whisper",
            "shout",
            "call out",
            "haggle",
            "barter",
            "confess",
            "accuse",
            "apologize",
            "plead",
            "taunt",
        ];
        if dialogue_words.iter().any(|w| Self::has_word(&lower, w)) {
            return IntentRoute::for_intent(Intent::Dialogue);
        }

        // Exploration keywords
        let explore_words = [
            "look", "go", "move", "walk", "enter", "explore", "search", "open", "travel",
        ];
        if explore_words.iter().any(|w| Self::has_word(&lower, w)) {
            return IntentRoute::for_intent(Intent::Exploration);
        }

        // Examine keywords
        let examine_words = ["examine", "inspect", "study", "read", "check"];
        if examine_words.iter().any(|w| Self::has_word(&lower, w)) {
            return IntentRoute::for_intent(Intent::Examine);
        }

        // Meta keywords
        let meta_words = ["save", "help", "status", "inventory", "quit"];
        if meta_words.iter().any(|w| Self::has_word(&lower, w)) {
            return IntentRoute::for_intent(Intent::Meta);
        }

        // Default fallback: Exploration with lower confidence
        IntentRoute::fallback()
    }

    /// Classify with state override — active combat/chase forces intent regardless of input.
    /// Emits a tracing span with semantic fields for agent telemetry (story 3-1).
    pub fn classify_with_state(input: &str, ctx: &crate::orchestrator::TurnContext) -> IntentRoute {
        // Compute route first
        let route = if ctx.in_chase {
            IntentRoute::with_classification(
                Intent::Chase,
                1.0,
                vec![],
                ClassificationSource::StateOverride,
            )
        } else if ctx.in_combat {
            IntentRoute::with_classification(
                Intent::Combat,
                1.0,
                vec![],
                ClassificationSource::StateOverride,
            )
        } else {
            Self::classify_keywords_inner(input)
        };

        let intent_str = format!("{:?}", route.intent());
        let span = tracing::info_span!(
            "classify_with_state",
            player_input = %input,
            classified_intent = %intent_str,
            agent_routed_to = %route.agent_name(),
            state_override = ctx.in_combat || ctx.in_chase,
            source = %route.source(),
            confidence = route.confidence(),
        );
        let _guard = span.enter();

        route
    }

    /// Two-tier classification pipeline (ADR-032).
    ///
    /// 1. State override (in_combat/in_chase) → immediate dispatch
    /// 2. Haiku classifier → if high confidence, dispatch; if ambiguous, return for narrator folding
    /// 3. Keyword fallback if classifier returns KeywordFallback source
    pub fn classify_two_tier(
        input: &str,
        ctx: &crate::orchestrator::TurnContext,
        classifier: &dyn IntentClassifier,
    ) -> IntentRoute {
        // Fast path: state overrides bypass everything
        if ctx.in_chase {
            return IntentRoute::with_classification(
                Intent::Chase,
                1.0,
                vec![],
                ClassificationSource::StateOverride,
            );
        }
        if ctx.in_combat {
            return IntentRoute::with_classification(
                Intent::Combat,
                1.0,
                vec![],
                ClassificationSource::StateOverride,
            );
        }

        // Tier 1: Haiku classifier
        let route = classifier.classify(input, ctx);

        // If the classifier returned a KeywordFallback source, it failed —
        // use our own keyword matching
        if route.source() == ClassificationSource::KeywordFallback {
            return Self::classify_keywords_inner(input);
        }

        // Return the Haiku result (high confidence or ambiguous)
        route
    }

    /// Add ambiguity context to the narrator prompt when classification is ambiguous (ADR-032).
    ///
    /// When Haiku returns low confidence, the candidates are folded into the narrator's
    /// prompt so it can resolve the ambiguity with full scene context.
    pub fn add_ambiguity_context(builder: &mut ContextBuilder, route: &IntentRoute) {
        if !route.is_ambiguous() {
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

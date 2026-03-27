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

    /// Create a route for a given intent (keyword fallback path).
    /// Confidence is 1.0 for direct matches, used by existing callers.
    pub fn for_intent(intent: Intent) -> Self {
        Self {
            agent_name: Self::agent_for(intent).to_string(),
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

    /// Parse the JSON response from Haiku into an IntentRoute.
    fn parse_response(raw: &str) -> Option<IntentRoute> {
        let value: serde_json::Value = serde_json::from_str(raw.trim()).ok()?;

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
                            "Haiku classifier returned unparseable response, falling back to keywords"
                        );
                        IntentRoute::with_classification(
                            Intent::Exploration,
                            0.0,
                            vec![],
                            ClassificationSource::KeywordFallback,
                        )
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Haiku classifier failed, falling back to keywords"
                );
                IntentRoute::with_classification(
                    Intent::Exploration,
                    0.0,
                    vec![],
                    ClassificationSource::KeywordFallback,
                )
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

    /// Classify using the real Haiku classifier wired into this router.
    ///
    /// This is the production entry point — called from the orchestrator's turn loop.
    pub fn classify(&self, input: &str, ctx: &crate::orchestrator::TurnContext) -> IntentRoute {
        Self::classify_two_tier(input, ctx, &self.classifier)
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
            let route = IntentRoute::with_classification(
                Intent::Chase,
                1.0,
                vec![],
                ClassificationSource::StateOverride,
            );
            Self::emit_two_tier_span(input, &route, false);
            return route;
        }
        if ctx.in_combat {
            let route = IntentRoute::with_classification(
                Intent::Combat,
                1.0,
                vec![],
                ClassificationSource::StateOverride,
            );
            Self::emit_two_tier_span(input, &route, false);
            return route;
        }

        // Tier 1: Haiku classifier
        let route = classifier.classify(input, ctx);

        // If the classifier returned a KeywordFallback source, it failed —
        // use our own keyword matching
        if route.source() == ClassificationSource::KeywordFallback {
            tracing::warn!(
                player_input = %input,
                "Haiku classifier degraded, falling back to keyword matching"
            );
            let fallback = Self::classify_keywords_inner(input);
            Self::emit_two_tier_span(input, &fallback, true);
            return fallback;
        }

        // Return the Haiku result (high confidence or ambiguous)
        Self::emit_two_tier_span(input, &route, false);
        route
    }

    /// Emit a tracing span for the two-tier classification pipeline.
    fn emit_two_tier_span(input: &str, route: &IntentRoute, haiku_degraded: bool) {
        let intent_str = format!("{:?}", route.intent());
        let _span = tracing::info_span!(
            "classify_two_tier",
            player_input = %input,
            classified_intent = %intent_str,
            agent_routed_to = %route.agent_name(),
            source = %route.source(),
            confidence = route.confidence(),
            is_ambiguous = route.is_ambiguous(),
            haiku_degraded = haiku_degraded,
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

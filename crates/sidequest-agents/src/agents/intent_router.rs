//! Intent router — state-based inference of player intent (ADR-067).
//!
//! ADR-067: Unified narrator agent. Intent classification no longer requires
//! an LLM call. Combat and chase are inferred from game state; everything
//! else goes to the narrator. The Intent enum and IntentRoute struct are
//! retained for OTEL telemetry and conditional prompt section injection.

/// Player intent categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
        matches!(
            self,
            Intent::Combat
                | Intent::Dialogue
                | Intent::Chase
                | Intent::Backstory
                | Intent::Accusation
        )
    }

    /// Reconstruct an Intent from its Display string representation.
    /// Used by dispatch to convert ActionResult's classified_intent (String)
    /// back to the typed enum.
    ///
    /// Returns `None` for unrecognized strings — callers MUST decide whether
    /// to default loudly (panic), default quietly (e.g., to `Exploration`),
    /// or hard-reject. The previous version of this function silently
    /// defaulted to `Intent::Exploration`, which created a hidden silent
    /// fallback that defeated the guest NPC permission gate added in
    /// story 35-6 (an unknown intent string would slip through as
    /// `Exploration → Movement` and bypass restrictions). Returning Option
    /// pushes the fallback decision to the call site where the policy is
    /// known.
    pub fn from_display_str(s: &str) -> Option<Self> {
        match s {
            "Combat" => Some(Intent::Combat),
            "Dialogue" => Some(Intent::Dialogue),
            "Exploration" => Some(Intent::Exploration),
            "Examine" => Some(Intent::Examine),
            "Meta" => Some(Intent::Meta),
            "Chase" => Some(Intent::Chase),
            "Backstory" => Some(Intent::Backstory),
            "Accusation" => Some(Intent::Accusation),
            _ => None,
        }
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
            return Err(format!("confidence must be 0.0..=1.0, got {confidence}"));
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

    /// All intents route to the narrator (ADR-067, story 28-6).
    ///
    /// This is the only classification path. No LLM call, no state inference,
    /// no keyword matching. Encounter context is injected into the narrator's
    /// prompt via conditional sections; the narrator's game_patch output drives
    /// encounter mechanics. Player beat selections arrive via the structured
    /// BEAT_SELECTION protocol message, not through intent classification.
    ///
    /// The `IntentRouter` struct, `IntentClassifier` trait, and `NoOpClassifier`
    /// were deleted in the confrontation wiring repair — they were dead code
    /// that unconditionally returned this same constant while pretending to
    /// classify. Per CLAUDE.md: "no stubs."
    pub fn exploration() -> Self {
        Self {
            agent_name: Self::agent_for(Intent::Exploration).to_string(),
            intent: Intent::Exploration,
            confidence: 1.0,
            candidates: vec![],
            source: ClassificationSource::StateOverride,
        }
    }
}

// IntentRouter, IntentClassifier, NoOpClassifier DELETED — confrontation
// wiring repair, 2026-04-12. The router was a dead stub that returned
// Intent::Exploration unconditionally since story 28-6 (ADR-067). The
// classifier trait had exactly one implementor (NoOpClassifier) that also
// returned Exploration. Callers now use IntentRoute::exploration() directly.

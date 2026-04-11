//! BeliefState — per-NPC knowledge bubbles for the Scenario System.
//!
//! Story 7-1: Each NPC maintains their own set of beliefs about the game world.
//! Beliefs come in three variants: Facts (confirmed knowledge), Suspicions
//! (uncertain beliefs with confidence), and Claims (statements by others that
//! may or may not be believed). NPCs also track credibility scores for other
//! NPCs, used to weight incoming information.

use serde::{Deserialize, Serialize};
use sidequest_telemetry::{WatcherEventBuilder, WatcherEventType};
use std::collections::HashMap;

/// Per-NPC knowledge container.
///
/// Holds a list of beliefs and a map of credibility scores for other NPCs.
/// Used by the Scenario System to track what each NPC knows, suspects, and
/// has been told.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeliefState {
    beliefs: Vec<Belief>,
    credibility_scores: HashMap<String, Credibility>,
}

impl BeliefState {
    /// Create an empty BeliefState.
    pub fn new() -> Self {
        Self {
            beliefs: Vec::new(),
            credibility_scores: HashMap::new(),
        }
    }

    /// Access the beliefs list.
    pub fn beliefs(&self) -> &[Belief] {
        &self.beliefs
    }

    /// Access the credibility scores map.
    pub fn credibility_scores(&self) -> &HashMap<String, Credibility> {
        &self.credibility_scores
    }

    /// Add a belief to this NPC's knowledge.
    pub fn add_belief(&mut self, belief: Belief) {
        // OTEL: belief_state.belief_added — GM panel verification that
        // beliefs are actually flowing into NPC knowledge (scenario engine,
        // dispatch pipeline, and gossip propagation all land here).
        let (variant, source_label) = belief_signature(&belief);
        WatcherEventBuilder::new("belief_state", WatcherEventType::StateTransition)
            .field("action", "belief_added")
            .field("variant", variant)
            .field("subject", belief.subject())
            .field("content", belief.content())
            .field("source", source_label)
            .field("turn_learned", belief.turn_learned())
            .field("beliefs_count_after", self.beliefs.len() + 1)
            .send();

        self.beliefs.push(belief);
    }

    /// Query beliefs about a specific subject (case-sensitive exact match).
    pub fn beliefs_about(&self, subject: &str) -> Vec<&Belief> {
        self.beliefs
            .iter()
            .filter(|b| b.subject() == subject)
            .collect()
    }

    /// Get the credibility score for a named NPC.
    /// Returns default (0.5) if the NPC has no recorded credibility.
    pub fn credibility_of(&self, npc_name: &str) -> Credibility {
        self.credibility_scores
            .get(npc_name)
            .copied()
            .unwrap_or_default()
    }

    /// Set the credibility score for a named NPC (clamped to 0.0..=1.0).
    pub fn update_credibility(&mut self, npc_name: &str, score: f32) {
        // OTEL: belief_state.credibility_updated — GM panel visibility into
        // trust-graph mutations. Captures pre/post clamp for debugging
        // decay chains from gossip contradictions.
        let previous = self.credibility_scores.get(npc_name).map(|c| c.score());
        let clamped = Credibility::new(score);
        WatcherEventBuilder::new("belief_state", WatcherEventType::StateTransition)
            .field("action", "credibility_updated")
            .field("target_npc", npc_name)
            .field("previous_score", previous)
            .field("requested_score", score)
            .field("new_score", clamped.score())
            .send();

        self.credibility_scores
            .insert(npc_name.to_string(), clamped);
    }
}

/// Extract `(variant_label, source_label)` from a belief for telemetry.
fn belief_signature(belief: &Belief) -> (&'static str, String) {
    let variant = match belief {
        Belief::Fact { .. } => "fact",
        Belief::Suspicion { .. } => "suspicion",
        Belief::Claim { .. } => "claim",
    };
    let source = match belief.source() {
        BeliefSource::Witnessed => "witnessed".to_string(),
        BeliefSource::ToldBy(name) => format!("told_by:{name}"),
        BeliefSource::Inferred => "inferred".to_string(),
        BeliefSource::Overheard => "overheard".to_string(),
    };
    (variant, source)
}

impl Default for BeliefState {
    fn default() -> Self {
        Self::new()
    }
}

/// A single belief held by an NPC.
///
/// Beliefs are tagged by variant: Facts are confirmed knowledge, Suspicions
/// carry a confidence level, and Claims track who said what and whether
/// the NPC believes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Belief {
    /// Confirmed knowledge — the NPC knows this for certain.
    Fact {
        subject: String,
        content: String,
        turn_learned: u64,
        source: BeliefSource,
    },
    /// Uncertain belief with a confidence level (0.0..=1.0).
    Suspicion {
        subject: String,
        content: String,
        turn_learned: u64,
        source: BeliefSource,
        confidence: f32,
    },
    /// A statement made by another NPC, which may or may not be believed.
    Claim {
        subject: String,
        content: String,
        turn_learned: u64,
        source: BeliefSource,
        believed: bool,
        /// Typed sentiment — set by the LLM when the belief is created.
        #[serde(default = "default_claim_sentiment")]
        sentiment: ClaimSentiment,
    },
}

impl Belief {
    /// The subject this belief is about.
    pub fn subject(&self) -> &str {
        match self {
            Belief::Fact { subject, .. } => subject,
            Belief::Suspicion { subject, .. } => subject,
            Belief::Claim { subject, .. } => subject,
        }
    }

    /// The content/description of this belief.
    pub fn content(&self) -> &str {
        match self {
            Belief::Fact { content, .. } => content,
            Belief::Suspicion { content, .. } => content,
            Belief::Claim { content, .. } => content,
        }
    }

    /// The turn number when this belief was learned.
    pub fn turn_learned(&self) -> u64 {
        match self {
            Belief::Fact { turn_learned, .. } => *turn_learned,
            Belief::Suspicion { turn_learned, .. } => *turn_learned,
            Belief::Claim { turn_learned, .. } => *turn_learned,
        }
    }

    /// The source of this belief.
    pub fn source(&self) -> &BeliefSource {
        match self {
            Belief::Fact { source, .. } => source,
            Belief::Suspicion { source, .. } => source,
            Belief::Claim { source, .. } => source,
        }
    }

    /// Create a Suspicion with confidence clamped to 0.0..=1.0.
    pub fn suspicion(
        subject: String,
        content: String,
        turn_learned: u64,
        source: BeliefSource,
        confidence: f32,
    ) -> Self {
        Belief::Suspicion {
            subject,
            content,
            turn_learned,
            source,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }
}

fn default_claim_sentiment() -> ClaimSentiment {
    ClaimSentiment::Neutral
}

/// Sentiment of a claim relative to an accusation — typed at creation, not keyword-parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimSentiment {
    /// Claim supports guilt / responsibility.
    Corroborating,
    /// Claim supports innocence / provides alibi.
    Contradicting,
    /// Claim is neutral / ambiguous regarding guilt.
    Neutral,
}

/// How the NPC acquired a belief.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BeliefSource {
    /// The NPC saw or sensed it directly.
    Witnessed,
    /// Told by a specific NPC (name stored).
    ToldBy(String),
    /// Deduced from available information.
    Inferred,
    /// Heard indirectly (eavesdropping, gossip).
    Overheard,
}

/// Trust score for another NPC, clamped to 0.0..=1.0.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Credibility(f32);

impl Credibility {
    /// Create a credibility score, clamped to 0.0..=1.0.
    pub fn new(score: f32) -> Self {
        Self(score.clamp(0.0, 1.0))
    }

    /// Get the credibility score.
    pub fn score(&self) -> f32 {
        self.0
    }

    /// Adjust the credibility by a delta, clamping to 0.0..=1.0.
    pub fn adjust(&mut self, delta: f32) {
        self.0 = (self.0 + delta).clamp(0.0, 1.0);
    }
}

impl Default for Credibility {
    fn default() -> Self {
        Self(0.5)
    }
}

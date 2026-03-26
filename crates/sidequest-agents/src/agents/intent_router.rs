//! Intent router — LLM-based classification of player input to agent.
//!
//! ADR-010: Intent-based agent routing. An LLM classifier routes each player
//! input to a specialist agent based on intent and current game state.

use crate::client::ClaudeClient;

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

/// A routing decision mapping an intent to an agent.
#[derive(Debug, Clone)]
pub struct IntentRoute {
    agent_name: String,
    intent: Intent,
}

impl IntentRoute {
    /// Create a route for a given intent.
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
        }
    }

    /// Fallback route — defaults to Narrator (ADR-010).
    pub fn fallback() -> Self {
        Self {
            agent_name: "narrator".to_string(),
            intent: Intent::Exploration,
        }
    }

    /// The agent name this route points to.
    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

    /// The classified intent.
    pub fn intent(&self) -> Intent {
        self.intent
    }
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
    /// This is the synchronous fast path. For ambiguous input, defaults to Exploration.
    pub fn classify_keywords(input: &str) -> IntentRoute {
        let lower = input.to_lowercase();

        // Combat keywords
        let combat_words = [
            "attack", "slash", "strike", "cast", "shoot", "defend", "stab",
            "fight", "hit", "swing", "parry", "block", "spell",
        ];
        if combat_words.iter().any(|w| lower.contains(w)) {
            return IntentRoute::for_intent(Intent::Combat);
        }

        // Dialogue keywords
        let dialogue_words = ["talk", "tell", "ask", "say", "speak", "greet", "persuade", "negotiate"];
        if dialogue_words.iter().any(|w| lower.contains(w)) {
            return IntentRoute::for_intent(Intent::Dialogue);
        }

        // Exploration keywords
        let explore_words = ["look", "go", "move", "walk", "enter", "explore", "search", "open", "travel"];
        if explore_words.iter().any(|w| lower.contains(w)) {
            return IntentRoute::for_intent(Intent::Exploration);
        }

        // Examine keywords
        let examine_words = ["examine", "inspect", "study", "read", "check"];
        if examine_words.iter().any(|w| lower.contains(w)) {
            return IntentRoute::for_intent(Intent::Examine);
        }

        // Meta keywords
        let meta_words = ["save", "help", "status", "inventory", "quit"];
        if meta_words.iter().any(|w| lower.contains(w)) {
            return IntentRoute::for_intent(Intent::Meta);
        }

        // Default fallback: Exploration
        IntentRoute::fallback()
    }

    /// Classify with state override — active combat/chase forces intent regardless of input.
    pub fn classify_with_state(input: &str, ctx: &crate::orchestrator::TurnContext) -> IntentRoute {
        // State overrides take priority
        if ctx.in_chase {
            return IntentRoute::for_intent(Intent::Chase);
        }
        if ctx.in_combat {
            return IntentRoute::for_intent(Intent::Combat);
        }

        // Fall through to keyword matching
        Self::classify_keywords(input)
    }
}

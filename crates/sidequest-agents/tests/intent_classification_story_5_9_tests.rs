//! Story 5-9: Intent classification tests (post-PR#95 refactor)
//!
//! ADR-032: Keyword classification with state overrides.
//! Tests the current IntentRouter API: classify_keywords and classify_with_state.

use sidequest_agents::agents::intent_router::{Intent, IntentRoute, IntentRouter};
use sidequest_agents::orchestrator::TurnContext;

// ============================================================================
// AC: IntentRoute data model
// ============================================================================

#[test]
fn intent_route_for_intent_has_correct_agent() {
    let route = IntentRoute::for_intent(Intent::Combat);
    assert_eq!(route.agent_name(), "creature_smith");
    assert_eq!(route.intent(), Intent::Combat);
}

#[test]
fn fallback_route_is_narrator_exploration() {
    let route = IntentRoute::fallback();
    assert_eq!(route.agent_name(), "narrator");
    assert_eq!(route.intent(), Intent::Exploration);
}

// ============================================================================
// AC: Keyword classification
// ============================================================================

#[test]
fn classify_keywords_combat() {
    let route = IntentRouter::classify_keywords("I attack the goblin");
    assert_eq!(route.intent(), Intent::Combat);
    assert_eq!(route.agent_name(), "creature_smith");
}

#[test]
fn classify_keywords_dialogue() {
    let route = IntentRouter::classify_keywords("I talk to the merchant");
    assert_eq!(route.intent(), Intent::Dialogue);
    assert_eq!(route.agent_name(), "ensemble");
}

#[test]
fn classify_keywords_exploration() {
    let route = IntentRouter::classify_keywords("I go to the tavern");
    assert_eq!(route.intent(), Intent::Exploration);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn classify_keywords_examine() {
    let route = IntentRouter::classify_keywords("I examine the strange markings");
    assert_eq!(route.intent(), Intent::Examine);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn classify_keywords_meta() {
    let route = IntentRouter::classify_keywords("save");
    assert_eq!(route.intent(), Intent::Meta);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn classify_keywords_fallback_on_unknown() {
    let route = IntentRouter::classify_keywords("I contemplate the meaning of existence");
    assert_eq!(route.intent(), Intent::Exploration);
    assert_eq!(route.agent_name(), "narrator");
}

// ============================================================================
// AC: State override bypasses keywords
// ============================================================================

#[test]
fn state_override_combat() {
    let ctx = TurnContext {
        in_combat: true,
        in_chase: false,
        state_summary: None,
    };
    let route = IntentRouter::classify_with_state("I look around", &ctx);
    assert_eq!(route.intent(), Intent::Combat);
}

#[test]
fn state_override_chase() {
    let ctx = TurnContext {
        in_combat: false,
        in_chase: true,
        state_summary: None,
    };
    let route = IntentRouter::classify_with_state("I talk to the merchant", &ctx);
    assert_eq!(route.intent(), Intent::Chase);
}

#[test]
fn chase_takes_priority_over_combat() {
    let ctx = TurnContext {
        in_combat: true,
        in_chase: true,
        state_summary: None,
    };
    let route = IntentRouter::classify_with_state("I attack", &ctx);
    assert_eq!(route.intent(), Intent::Chase, "Chase should take priority over combat");
}

#[test]
fn no_state_override_falls_through_to_keywords() {
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_with_state("I attack the goblin", &ctx);
    assert_eq!(route.intent(), Intent::Combat);
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn empty_input_classifies_without_panic() {
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_with_state("", &ctx);
    assert_eq!(route.intent(), Intent::Exploration, "Empty input should fallback");
}

#[test]
fn very_long_input_classifies_without_panic() {
    let long_input = "I ".to_string() + &"really ".repeat(1000) + "want to attack";
    let route = IntentRouter::classify_keywords(&long_input);
    assert_eq!(route.intent(), Intent::Combat);
}

#[test]
fn classify_does_not_mutate_context() {
    let ctx = TurnContext::default();
    let _route = IntentRouter::classify_with_state("attack", &ctx);
    assert!(!ctx.in_combat, "classify must not mutate state");
    assert!(!ctx.in_chase, "classify must not mutate state");
}

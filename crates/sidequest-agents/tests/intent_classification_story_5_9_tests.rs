//! Story 5-9: Intent classification tests (post-ADR-032 refactor)
//!
//! ADR-032: Haiku classifier with state overrides and narrator fallback.
//! Tests the IntentRouter API: classify_with_classifier, state overrides,
//! and narrator_fallback.

use sidequest_agents::agents::intent_router::{
    Intent, IntentClassifier, IntentRoute, IntentRouter,
};
use sidequest_agents::orchestrator::TurnContext;

/// Mock classifier that always returns a fixed intent.
struct MockClassifier(Intent);

impl IntentClassifier for MockClassifier {
    fn classify(&self, _input: &str, _context: &TurnContext) -> IntentRoute {
        IntentRoute::for_intent(self.0)
    }
}

// ============================================================================
// AC: IntentRoute data model
// ============================================================================

#[test]
fn intent_route_for_intent_has_correct_agent() {
    // ADR-067: All intents route to narrator
    let route = IntentRoute::for_intent(Intent::Combat);
    assert_eq!(route.agent_name(), "narrator");
    assert_eq!(route.intent(), Intent::Combat);
}

#[test]
fn fallback_route_is_narrator_exploration() {
    let route = IntentRoute::narrator_fallback();
    assert_eq!(route.agent_name(), "narrator");
    assert_eq!(route.intent(), Intent::Exploration);
}

// ============================================================================
// AC: Mock classifier returns correct routing
// ============================================================================

#[test]
fn classify_with_classifier_combat() {
    let ctx = TurnContext::default();
    let classifier = MockClassifier(Intent::Combat);
    let route = IntentRouter::classify_with_classifier("I attack the goblin", &ctx, &classifier);
    // ADR-067: State-based inference, no LLM. Default is Exploration.
    assert_eq!(route.intent(), Intent::Exploration);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn classify_with_classifier_dialogue() {
    let ctx = TurnContext::default();
    let classifier = MockClassifier(Intent::Dialogue);
    let route = IntentRouter::classify_with_classifier("I talk to the merchant", &ctx, &classifier);
    // ADR-067: All intents route to narrator, state-based defaults to Exploration
    assert_eq!(route.intent(), Intent::Exploration);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn classify_with_classifier_exploration() {
    let ctx = TurnContext::default();
    let classifier = MockClassifier(Intent::Exploration);
    let route = IntentRouter::classify_with_classifier("I go to the tavern", &ctx, &classifier);
    assert_eq!(route.intent(), Intent::Exploration);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn classify_with_classifier_examine() {
    // ADR-067: State-based inference ignores classifier, defaults to Exploration
    let ctx = TurnContext::default();
    let classifier = MockClassifier(Intent::Examine);
    let route =
        IntentRouter::classify_with_classifier("I examine the strange markings", &ctx, &classifier);
    assert_eq!(route.intent(), Intent::Exploration);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn classify_with_classifier_meta() {
    // ADR-067: State-based inference ignores classifier, defaults to Exploration
    let ctx = TurnContext::default();
    let classifier = MockClassifier(Intent::Meta);
    let route = IntentRouter::classify_with_classifier("save", &ctx, &classifier);
    assert_eq!(route.intent(), Intent::Exploration);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn classify_with_classifier_fallback_intent() {
    let ctx = TurnContext::default();
    let classifier = MockClassifier(Intent::Exploration);
    let route = IntentRouter::classify_with_classifier(
        "I contemplate the meaning of existence",
        &ctx,
        &classifier,
    );
    assert_eq!(route.intent(), Intent::Exploration);
    assert_eq!(route.agent_name(), "narrator");
}

// ============================================================================
// AC: ADR-067 / Story 28-6 — unified narrator routing
//
// The intent router was simplified in ADR-067: every action routes to the
// narrator with Intent::Exploration. Encounters are still recognised, but the
// narrator handles them via beat_selections injected into the game_patch
// output instead of dispatching to a separate Combat/Chase agent. The tests
// below pin that architecture so we don't drift back into split routing.
// ============================================================================

#[test]
fn in_combat_state_still_routes_to_narrator() {
    let ctx = TurnContext {
        in_combat: true,
        in_chase: false,
        state_summary: None,
        ..Default::default()
    };
    let classifier = MockClassifier(Intent::Exploration);
    let route = IntentRouter::classify_with_classifier("I attack the goblin", &ctx, &classifier);
    assert_eq!(
        route.intent(),
        Intent::Exploration,
        "ADR-067: in_combat does not branch — narrator handles encounters via beat_selections"
    );
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn in_chase_state_still_routes_to_narrator() {
    let ctx = TurnContext {
        in_combat: false,
        in_chase: true,
        state_summary: None,
        ..Default::default()
    };
    let classifier = MockClassifier(Intent::Dialogue);
    let route = IntentRouter::classify_with_classifier("I run for the alley", &ctx, &classifier);
    assert_eq!(
        route.intent(),
        Intent::Exploration,
        "ADR-067: in_chase does not branch — narrator handles chase beats via beat_selections"
    );
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn combat_and_chase_simultaneously_still_routes_to_narrator() {
    let ctx = TurnContext {
        in_combat: true,
        in_chase: true,
        state_summary: None,
        ..Default::default()
    };
    let classifier = MockClassifier(Intent::Combat);
    let route = IntentRouter::classify_with_classifier("I attack while running", &ctx, &classifier);
    assert_eq!(
        route.intent(),
        Intent::Exploration,
        "ADR-067: combined encounter states still defer to the narrator"
    );
}

#[test]
fn no_state_override_defaults_to_exploration() {
    // ADR-067: Without state overrides, defaults to Exploration (narrator handles all)
    let ctx = TurnContext::default();
    let classifier = MockClassifier(Intent::Combat);
    let route = IntentRouter::classify_with_classifier("I attack the goblin", &ctx, &classifier);
    assert_eq!(route.intent(), Intent::Exploration);
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn empty_input_classifies_without_panic() {
    let ctx = TurnContext::default();
    let classifier = MockClassifier(Intent::Exploration);
    let route = IntentRouter::classify_with_classifier("", &ctx, &classifier);
    assert_eq!(
        route.intent(),
        Intent::Exploration,
        "Empty input should fallback"
    );
}

#[test]
fn very_long_input_classifies_without_panic() {
    // ADR-067: State-based inference, defaults to Exploration
    let long_input = "I ".to_string() + &"really ".repeat(1000) + "want to attack";
    let ctx = TurnContext::default();
    let classifier = MockClassifier(Intent::Combat);
    let route = IntentRouter::classify_with_classifier(&long_input, &ctx, &classifier);
    assert_eq!(route.intent(), Intent::Exploration);
}

#[test]
fn classify_does_not_mutate_context() {
    let ctx = TurnContext::default();
    let classifier = MockClassifier(Intent::Combat);
    let _route = IntentRouter::classify_with_classifier("attack", &ctx, &classifier);
    assert!(!ctx.in_combat, "classify must not mutate state");
    assert!(!ctx.in_chase, "classify must not mutate state");
}

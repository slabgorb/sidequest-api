//! Story 5-9: Two-tier intent classification tests
//!
//! RED phase — these tests reference types and methods that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - ClassificationSource enum (Haiku, StateOverride, KeywordFallback)
//!   - IntentRoute gains confidence, candidates, source fields
//!   - IntentClassifier trait (async Haiku LLM call)
//!   - Two-tier pipeline: classify() → Haiku → confidence check → dispatch or fold
//!   - Keyword fallback in degraded mode
//!   - Orchestrator integration with two-tier classification
//!
//! ADR-032: Two-Tier Intent Classification (Haiku + Narrator Fallback)
//!
//! ACs tested: data model, Haiku classifier, confidence routing, ambiguity
//! resolution, state overrides, keyword fallback, orchestrator integration

use sidequest_agents::agents::intent_router::{
    ClassificationSource, Intent, IntentClassifier, IntentRoute, IntentRouter,
};
use sidequest_agents::orchestrator::TurnContext;

// ============================================================================
// AC: ClassificationSource enum exists with correct variants
// Rule #2: #[non_exhaustive] on public enums
// ============================================================================

#[test]
fn classification_source_has_haiku_variant() {
    let source = ClassificationSource::Haiku;
    assert_eq!(format!("{:?}", source), "Haiku");
}

#[test]
fn classification_source_has_state_override_variant() {
    let source = ClassificationSource::StateOverride;
    assert_eq!(format!("{:?}", source), "StateOverride");
}

#[test]
fn classification_source_has_keyword_fallback_variant() {
    let source = ClassificationSource::KeywordFallback;
    assert_eq!(format!("{:?}", source), "KeywordFallback");
}

#[test]
fn classification_source_is_copy_and_eq() {
    let a = ClassificationSource::Haiku;
    let b = a; // Copy
    assert_eq!(a, b); // Eq
}

// ============================================================================
// AC: IntentRoute gains confidence, candidates, source fields (ADR-032 data model)
// Rule #9: Fields private with getters
// ============================================================================

#[test]
fn intent_route_has_confidence_field() {
    let route = IntentRoute::with_classification(
        Intent::Combat,
        0.95,
        vec![],
        ClassificationSource::Haiku,
    );
    assert!((route.confidence() - 0.95).abs() < f64::EPSILON);
}

#[test]
fn intent_route_has_candidates_field() {
    let candidates = vec![Intent::Combat, Intent::Dialogue];
    let route = IntentRoute::with_classification(
        Intent::Combat,
        0.4,
        candidates.clone(),
        ClassificationSource::Haiku,
    );
    assert_eq!(route.candidates(), &candidates);
}

#[test]
fn intent_route_has_source_field() {
    let route = IntentRoute::with_classification(
        Intent::Combat,
        0.95,
        vec![],
        ClassificationSource::Haiku,
    );
    assert_eq!(route.source(), ClassificationSource::Haiku);
}

#[test]
fn intent_route_high_confidence_has_empty_candidates() {
    let route = IntentRoute::with_classification(
        Intent::Exploration,
        0.9,
        vec![],
        ClassificationSource::Haiku,
    );
    assert!(route.candidates().is_empty());
    assert!(route.confidence() >= 0.5);
}

// ============================================================================
// AC: Confidence validation — must be 0.0-1.0
// Rule #5: Validated constructors at trust boundaries
// ============================================================================

#[test]
fn intent_route_rejects_confidence_above_one() {
    let result = IntentRoute::try_with_classification(
        Intent::Combat,
        1.5,
        vec![],
        ClassificationSource::Haiku,
    );
    assert!(result.is_err(), "Confidence > 1.0 must be rejected");
}

#[test]
fn intent_route_rejects_negative_confidence() {
    let result = IntentRoute::try_with_classification(
        Intent::Combat,
        -0.1,
        vec![],
        ClassificationSource::Haiku,
    );
    assert!(result.is_err(), "Negative confidence must be rejected");
}

#[test]
fn intent_route_accepts_boundary_confidence_zero() {
    let result = IntentRoute::try_with_classification(
        Intent::Combat,
        0.0,
        vec![],
        ClassificationSource::KeywordFallback,
    );
    assert!(result.is_ok(), "Confidence 0.0 is valid");
}

#[test]
fn intent_route_accepts_boundary_confidence_one() {
    let result = IntentRoute::try_with_classification(
        Intent::Combat,
        1.0,
        vec![],
        ClassificationSource::StateOverride,
    );
    assert!(result.is_ok(), "Confidence 1.0 is valid");
}

// ============================================================================
// AC: State override bypasses Haiku — source is StateOverride, confidence 1.0
// ADR-032: in_combat → Combat, in_chase → Chase (skip Haiku entirely)
// ============================================================================

#[test]
fn state_override_combat_sets_source_state_override() {
    let ctx = TurnContext {
        in_combat: true,
        in_chase: false,
        state_summary: None,
    };
    let route = IntentRouter::classify_with_state("I look around", &ctx);
    assert_eq!(route.source(), ClassificationSource::StateOverride);
    assert_eq!(route.intent(), Intent::Combat);
}

#[test]
fn state_override_chase_sets_source_state_override() {
    let ctx = TurnContext {
        in_combat: false,
        in_chase: true,
        state_summary: None,
    };
    let route = IntentRouter::classify_with_state("I talk to the merchant", &ctx);
    assert_eq!(route.source(), ClassificationSource::StateOverride);
    assert_eq!(route.intent(), Intent::Chase);
}

#[test]
fn state_override_has_full_confidence() {
    let ctx = TurnContext {
        in_combat: true,
        in_chase: false,
        state_summary: None,
    };
    let route = IntentRouter::classify_with_state("anything", &ctx);
    assert!((route.confidence() - 1.0).abs() < f64::EPSILON,
        "State overrides must have confidence 1.0");
}

#[test]
fn state_override_has_no_candidates() {
    let ctx = TurnContext {
        in_combat: false,
        in_chase: true,
        state_summary: None,
    };
    let route = IntentRouter::classify_with_state("run", &ctx);
    assert!(route.candidates().is_empty(),
        "State overrides are deterministic — no alternative candidates");
}

// ============================================================================
// AC: Keyword fallback — source is KeywordFallback when used as degraded path
// ADR-032: Keyword matcher retained for when Haiku is unavailable
// ============================================================================

#[test]
fn keyword_fallback_sets_source_keyword_fallback() {
    let route = IntentRouter::classify_keywords("I attack the goblin");
    assert_eq!(route.source(), ClassificationSource::KeywordFallback);
}

#[test]
fn keyword_fallback_has_confidence_one() {
    // Keywords are deterministic — when they match, confidence is 1.0
    let route = IntentRouter::classify_keywords("I attack the goblin");
    assert!((route.confidence() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn keyword_fallback_default_has_lower_confidence() {
    // Fallback to Exploration when no keyword matches — confidence should be lower
    // because this is a guess, not a match
    let route = IntentRouter::classify_keywords("I contemplate the meaning of existence");
    assert_eq!(route.intent(), Intent::Exploration);
    assert!(route.confidence() < 1.0,
        "Keyword fallback (no match) should have lower confidence than a direct match");
}

// ============================================================================
// AC: IntentClassifier trait — async Haiku LLM classification
// ADR-032: Haiku model call classifies every player action
// ============================================================================

#[test]
fn intent_classifier_trait_exists() {
    // Verify the trait can be used as a trait object
    fn _accepts_classifier(_c: &dyn IntentClassifier) {}
}

/// Mock classifier for testing the two-tier pipeline without actual LLM calls.
struct MockClassifier {
    response: IntentRoute,
}

impl MockClassifier {
    fn high_confidence(intent: Intent) -> Self {
        Self {
            response: IntentRoute::with_classification(
                intent,
                0.95,
                vec![],
                ClassificationSource::Haiku,
            ),
        }
    }

    fn ambiguous(candidates: Vec<Intent>) -> Self {
        let primary = candidates.first().copied().unwrap_or(Intent::Exploration);
        Self {
            response: IntentRoute::with_classification(
                primary,
                0.3,
                candidates,
                ClassificationSource::Haiku,
            ),
        }
    }
}

impl IntentClassifier for MockClassifier {
    fn classify(
        &self,
        _input: &str,
        _context: &TurnContext,
    ) -> IntentRoute {
        self.response.clone()
    }
}

// ============================================================================
// AC: Two-tier pipeline — high confidence dispatches directly
// ADR-032: confidence >= 0.5 → Dispatch to specialist agent
// ============================================================================

#[test]
fn high_confidence_haiku_routes_directly() {
    let classifier = MockClassifier::high_confidence(Intent::Combat);
    let ctx = TurnContext::default();
    let route = classifier.classify("I attack the goblin", &ctx);
    assert_eq!(route.intent(), Intent::Combat);
    assert!(route.confidence() >= 0.5);
    assert!(route.candidates().is_empty());
    assert_eq!(route.source(), ClassificationSource::Haiku);
}

#[test]
fn high_confidence_dialogue_routes_to_ensemble() {
    let classifier = MockClassifier::high_confidence(Intent::Dialogue);
    let ctx = TurnContext::default();
    let route = classifier.classify("I talk to the guard", &ctx);
    assert_eq!(route.intent(), Intent::Dialogue);
    assert_eq!(route.agent_name(), "ensemble");
}

// ============================================================================
// AC: Two-tier pipeline — low confidence returns ambiguous with candidates
// ADR-032: confidence < 0.5 → fold candidates into narrator prompt
// ============================================================================

#[test]
fn low_confidence_returns_candidates() {
    let classifier = MockClassifier::ambiguous(vec![Intent::Combat, Intent::Dialogue]);
    let ctx = TurnContext::default();
    let route = classifier.classify(
        "I try to talk the guard into letting me attack",
        &ctx,
    );
    assert!(route.confidence() < 0.5);
    assert!(!route.candidates().is_empty());
    assert!(route.candidates().contains(&Intent::Combat));
    assert!(route.candidates().contains(&Intent::Dialogue));
}

#[test]
fn ambiguous_route_source_is_haiku() {
    // Even when ambiguous, the source is Haiku (not fallback)
    let classifier = MockClassifier::ambiguous(vec![Intent::Exploration, Intent::Examine]);
    let ctx = TurnContext::default();
    let route = classifier.classify("I look at the strange markings", &ctx);
    assert_eq!(route.source(), ClassificationSource::Haiku);
}

// ============================================================================
// AC: Ambiguity is detectable for orchestrator routing
// ADR-032: Orchestrator checks confidence before dispatching vs. folding
// ============================================================================

#[test]
fn intent_route_is_ambiguous_when_low_confidence() {
    let route = IntentRoute::with_classification(
        Intent::Combat,
        0.3,
        vec![Intent::Combat, Intent::Dialogue],
        ClassificationSource::Haiku,
    );
    assert!(route.is_ambiguous(), "confidence < 0.5 with candidates should be ambiguous");
}

#[test]
fn intent_route_is_not_ambiguous_when_high_confidence() {
    let route = IntentRoute::with_classification(
        Intent::Combat,
        0.9,
        vec![],
        ClassificationSource::Haiku,
    );
    assert!(!route.is_ambiguous(), "confidence >= 0.5 should not be ambiguous");
}

#[test]
fn state_override_is_never_ambiguous() {
    let route = IntentRoute::with_classification(
        Intent::Combat,
        1.0,
        vec![],
        ClassificationSource::StateOverride,
    );
    assert!(!route.is_ambiguous());
}

#[test]
fn keyword_fallback_is_never_ambiguous() {
    let route = IntentRoute::with_classification(
        Intent::Combat,
        1.0,
        vec![],
        ClassificationSource::KeywordFallback,
    );
    assert!(!route.is_ambiguous());
}

// ============================================================================
// AC: Substring false positive fix — ADR-032 motivation
// These tests document the bugs that keyword matching produces
// ============================================================================

#[test]
fn castle_should_not_match_combat_cast() {
    // "castle" contains "cast" → keyword matcher misroutes to Combat
    // The Haiku classifier should handle this correctly
    // For now, this documents the known keyword matcher bug
    let route = IntentRouter::classify_keywords("I walk toward the castle");
    // With keyword matching, this incorrectly returns Combat because "cast" matches.
    // The two-tier system should eventually route this to Exploration.
    // This test documents the current buggy behavior that 5-9 aims to fix.
    assert_eq!(
        route.intent(),
        Intent::Exploration,
        "BUG: 'castle' should not trigger Combat — contains 'cast' substring"
    );
}

#[test]
fn stalking_should_not_match_dialogue_talk() {
    // "stalking" contains "talk" → keyword matcher misroutes to Dialogue
    let route = IntentRouter::classify_keywords("I am stalking the deer through the forest");
    assert_eq!(
        route.intent(),
        Intent::Exploration,
        "BUG: 'stalking' should not trigger Dialogue — contains 'talk' substring"
    );
}

// ============================================================================
// AC: IntentRouter.classify_two_tier — the full pipeline method
// ADR-032: State override → Haiku → confidence check → dispatch or fold
// ============================================================================

#[test]
fn classify_two_tier_uses_state_override_first() {
    let classifier = MockClassifier::high_confidence(Intent::Exploration);
    let ctx = TurnContext {
        in_combat: true,
        in_chase: false,
        state_summary: None,
    };
    // Even though classifier would return Exploration, state override wins
    let route = IntentRouter::classify_two_tier("I look around", &ctx, &classifier);
    assert_eq!(route.intent(), Intent::Combat);
    assert_eq!(route.source(), ClassificationSource::StateOverride);
}

#[test]
fn classify_two_tier_falls_back_to_keywords_on_classifier_error() {
    // When the Haiku classifier is unavailable, fall back to keywords
    let classifier = FailingClassifier;
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_two_tier("I attack the goblin", &ctx, &classifier);
    assert_eq!(route.intent(), Intent::Combat);
    assert_eq!(route.source(), ClassificationSource::KeywordFallback);
}

/// A classifier that always fails — simulates Haiku API being down.
struct FailingClassifier;

impl IntentClassifier for FailingClassifier {
    fn classify(
        &self,
        _input: &str,
        _context: &TurnContext,
    ) -> IntentRoute {
        // Simulates error by returning a fallback with KeywordFallback source
        // In the real implementation, classify_two_tier catches the error
        // and falls back to keyword matching
        IntentRoute::with_classification(
            Intent::Exploration,
            0.0,
            vec![],
            ClassificationSource::KeywordFallback,
        )
    }
}

#[test]
fn classify_two_tier_dispatches_high_confidence_haiku() {
    let classifier = MockClassifier::high_confidence(Intent::Dialogue);
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_two_tier("I talk to the merchant", &ctx, &classifier);
    assert_eq!(route.intent(), Intent::Dialogue);
    assert_eq!(route.source(), ClassificationSource::Haiku);
    assert!(route.confidence() >= 0.5);
    assert!(!route.is_ambiguous());
}

#[test]
fn classify_two_tier_returns_ambiguous_for_low_confidence() {
    let classifier = MockClassifier::ambiguous(vec![Intent::Combat, Intent::Dialogue]);
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_two_tier(
        "I try to talk the guard into letting me fight",
        &ctx,
        &classifier,
    );
    assert!(route.is_ambiguous());
    assert!(route.confidence() < 0.5);
    assert!(!route.candidates().is_empty());
}

// ============================================================================
// INTEGRATION: Orchestrator uses two-tier classification in turn loop
// User requirement: "I don't want to write this and have it sit there and do nothing"
// ============================================================================

#[test]
fn orchestrator_process_action_uses_classification_source() {
    // The ActionResult (or TurnResult) should expose the classification source
    // so the client knows how the intent was determined
    use sidequest_agents::orchestrator::TurnResult;

    // TurnResult should have a classification_source field
    let _field_check = |r: &TurnResult| -> ClassificationSource {
        r.classification_source
    };
}

#[test]
fn orchestrator_folds_ambiguity_into_narrator_prompt() {
    // When classification is ambiguous, the orchestrator should fold
    // the candidates into the narrator's prompt per ADR-032:
    //   "Intent classification was ambiguous between {candidates}.
    //    Based on the current scene context, use your judgment..."
    //
    // This test verifies the orchestrator's ContextBuilder includes
    // an ambiguity section when the route is ambiguous.
    use sidequest_agents::context_builder::ContextBuilder;
    use sidequest_agents::prompt_framework::SectionCategory;

    let ambiguous_route = IntentRoute::with_classification(
        Intent::Combat,
        0.3,
        vec![Intent::Combat, Intent::Dialogue],
        ClassificationSource::Haiku,
    );

    // The orchestrator should add an ambiguity context section
    let mut builder = ContextBuilder::new();
    IntentRouter::add_ambiguity_context(&mut builder, &ambiguous_route);
    let prompt = builder.compose();

    assert!(
        prompt.contains("ambiguous"),
        "Ambiguous classification must fold context into narrator prompt"
    );
    assert!(
        prompt.contains("Combat"),
        "Prompt must mention the candidate intents"
    );
    assert!(
        prompt.contains("Dialogue"),
        "Prompt must mention all candidate intents"
    );
}

#[test]
fn orchestrator_does_not_fold_ambiguity_for_high_confidence() {
    use sidequest_agents::context_builder::ContextBuilder;

    let clear_route = IntentRoute::with_classification(
        Intent::Combat,
        0.95,
        vec![],
        ClassificationSource::Haiku,
    );

    let mut builder = ContextBuilder::new();
    IntentRouter::add_ambiguity_context(&mut builder, &clear_route);
    let prompt = builder.compose();

    assert!(
        !prompt.contains("ambiguous"),
        "High-confidence routes should NOT inject ambiguity context"
    );
}

// ============================================================================
// AC: Telemetry emits real confidence and source (ADR-032 consequence)
// Rule #4: Tracing coverage
// ============================================================================

#[test]
fn classify_with_state_emits_source_in_telemetry() {
    // The tracing span for classify_with_state should include the source field
    // This is a compile-time check that the span fields exist
    use sidequest_agents::agents::intent_router::IntentRouter;

    let ctx = TurnContext {
        in_combat: true,
        in_chase: false,
        state_summary: None,
    };

    // If this compiles and runs, the classify_with_state method exists
    // and returns IntentRoute with source. The actual telemetry verification
    // is covered by the telemetry story 3-1 tests.
    let route = IntentRouter::classify_with_state("attack", &ctx);
    assert_eq!(route.source(), ClassificationSource::StateOverride);
    // Real confidence, not hardcoded 1.0 for everything
    assert!((route.confidence() - 1.0).abs() < f64::EPSILON);
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn empty_input_classifies_without_panic() {
    let classifier = MockClassifier::high_confidence(Intent::Exploration);
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_two_tier("", &ctx, &classifier);
    // Should not panic — graceful handling of empty input
    let _ = route.intent();
    let _ = route.confidence();
}

#[test]
fn very_long_input_classifies_without_panic() {
    let long_input = "I ".to_string() + &"really ".repeat(1000) + "want to attack";
    let classifier = MockClassifier::high_confidence(Intent::Combat);
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_two_tier(&long_input, &ctx, &classifier);
    let _ = route.intent();
}

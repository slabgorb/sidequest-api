//! Story 5-9: Two-tier intent classification tests
//!
//! ADR-032: Haiku classifier with narrator ambiguity resolution.
//! When Haiku is unavailable, the narrator handles intent directly — no keyword fallback.
//!
//! ACs tested: data model, Haiku classifier, confidence routing, ambiguity
//! resolution, state overrides, narrator fallback, orchestrator integration

use sidequest_agents::agents::intent_router::{
    ClassificationSource, Intent, IntentClassifier, IntentRoute, IntentRouter,
};
use sidequest_agents::orchestrator::TurnContext;

// ============================================================================
// AC: ClassificationSource enum exists with correct variants
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
fn classification_source_has_haiku_unavailable_variant() {
    let source = ClassificationSource::HaikuUnavailable;
    assert_eq!(format!("{:?}", source), "HaikuUnavailable");
}

#[test]
fn classification_source_is_copy_and_eq() {
    let a = ClassificationSource::Haiku;
    let b = a; // Copy
    assert_eq!(a, b); // Eq
}

// ============================================================================
// AC: IntentRoute data model (ADR-032)
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
        ClassificationSource::HaikuUnavailable,
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
// AC: State override bypasses Haiku
// ============================================================================

#[test]
fn state_override_combat() {
    let mock = MockClassifier::high_confidence(Intent::Exploration);
    let ctx = TurnContext {
        in_combat: true,
        in_chase: false,
        state_summary: None,
    };
    let route = IntentRouter::classify_with_classifier("I look around", &ctx, &mock);
    assert_eq!(route.source(), ClassificationSource::StateOverride);
    assert_eq!(route.intent(), Intent::Combat);
}

#[test]
fn state_override_chase() {
    let mock = MockClassifier::high_confidence(Intent::Exploration);
    let ctx = TurnContext {
        in_combat: false,
        in_chase: true,
        state_summary: None,
    };
    let route = IntentRouter::classify_with_classifier("I talk to the merchant", &ctx, &mock);
    assert_eq!(route.source(), ClassificationSource::StateOverride);
    assert_eq!(route.intent(), Intent::Chase);
}

#[test]
fn state_override_has_full_confidence() {
    let mock = MockClassifier::high_confidence(Intent::Exploration);
    let ctx = TurnContext {
        in_combat: true,
        in_chase: false,
        state_summary: None,
    };
    let route = IntentRouter::classify_with_classifier("anything", &ctx, &mock);
    assert!((route.confidence() - 1.0).abs() < f64::EPSILON,
        "State overrides must have confidence 1.0");
}

#[test]
fn state_override_has_no_candidates() {
    let mock = MockClassifier::high_confidence(Intent::Exploration);
    let ctx = TurnContext {
        in_combat: false,
        in_chase: true,
        state_summary: None,
    };
    let route = IntentRouter::classify_with_classifier("run", &ctx, &mock);
    assert!(route.candidates().is_empty(),
        "State overrides are deterministic — no alternative candidates");
}

// ============================================================================
// AC: Narrator fallback when Haiku is unavailable
// ============================================================================

#[test]
fn narrator_fallback_routes_to_narrator() {
    let route = IntentRoute::narrator_fallback();
    assert_eq!(route.agent_name(), "narrator");
    assert_eq!(route.source(), ClassificationSource::HaikuUnavailable);
    assert_eq!(route.confidence(), 0.0);
}

#[test]
fn haiku_unavailable_routes_to_narrator() {
    let classifier = FailingClassifier;
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_with_classifier("I attack the goblin", &ctx, &classifier);
    assert_eq!(route.agent_name(), "narrator",
        "When Haiku is down, narrator handles everything");
    assert_eq!(route.source(), ClassificationSource::HaikuUnavailable);
}

// ============================================================================
// Mock classifiers
// ============================================================================

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
    fn classify(&self, _input: &str, _context: &TurnContext) -> IntentRoute {
        self.response.clone()
    }
}

/// Simulates Haiku being completely unavailable.
struct FailingClassifier;

impl IntentClassifier for FailingClassifier {
    fn classify(&self, _input: &str, _context: &TurnContext) -> IntentRoute {
        IntentRoute::narrator_fallback()
    }
}

// ============================================================================
// AC: Two-tier pipeline — high confidence dispatches directly
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
    let classifier = MockClassifier::ambiguous(vec![Intent::Exploration, Intent::Examine]);
    let ctx = TurnContext::default();
    let route = classifier.classify("I look at the strange markings", &ctx);
    assert_eq!(route.source(), ClassificationSource::Haiku);
}

// ============================================================================
// AC: Ambiguity detection
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
fn haiku_unavailable_is_never_ambiguous() {
    let route = IntentRoute::with_classification(
        Intent::Exploration,
        0.0,
        vec![],
        ClassificationSource::HaikuUnavailable,
    );
    assert!(!route.is_ambiguous(),
        "HaikuUnavailable is not ambiguous — it's failed, narrator handles it");
}

// ============================================================================
// AC: classify_with_classifier — the full pipeline method
// ============================================================================

#[test]
fn classify_with_classifier_uses_state_override_first() {
    let classifier = MockClassifier::high_confidence(Intent::Exploration);
    let ctx = TurnContext {
        in_combat: true,
        in_chase: false,
        state_summary: None,
    };
    let route = IntentRouter::classify_with_classifier("I look around", &ctx, &classifier);
    assert_eq!(route.intent(), Intent::Combat);
    assert_eq!(route.source(), ClassificationSource::StateOverride);
}

#[test]
fn classify_with_classifier_narrator_fallback_on_haiku_failure() {
    let classifier = FailingClassifier;
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_with_classifier("I attack the goblin", &ctx, &classifier);
    assert_eq!(route.agent_name(), "narrator");
    assert_eq!(route.source(), ClassificationSource::HaikuUnavailable);
}

#[test]
fn classify_with_classifier_dispatches_high_confidence_haiku() {
    let classifier = MockClassifier::high_confidence(Intent::Dialogue);
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_with_classifier("I talk to the merchant", &ctx, &classifier);
    assert_eq!(route.intent(), Intent::Dialogue);
    assert_eq!(route.source(), ClassificationSource::Haiku);
    assert!(route.confidence() >= 0.5);
    assert!(!route.is_ambiguous());
}

#[test]
fn classify_with_classifier_returns_ambiguous_for_low_confidence() {
    let classifier = MockClassifier::ambiguous(vec![Intent::Combat, Intent::Dialogue]);
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_with_classifier(
        "I try to talk the guard into letting me fight",
        &ctx,
        &classifier,
    );
    assert!(route.is_ambiguous());
    assert!(route.confidence() < 0.5);
    assert!(!route.candidates().is_empty());
}

// ============================================================================
// INTEGRATION: Orchestrator ambiguity folding
// ============================================================================

#[test]
fn orchestrator_process_action_uses_classification_source() {
    use sidequest_agents::orchestrator::TurnResult;

    let _field_check = |r: &TurnResult| -> ClassificationSource {
        r.classification_source
    };
}

#[test]
fn orchestrator_folds_ambiguity_into_narrator_prompt() {
    use sidequest_agents::context_builder::ContextBuilder;

    let ambiguous_route = IntentRoute::with_classification(
        Intent::Combat,
        0.3,
        vec![Intent::Combat, Intent::Dialogue],
        ClassificationSource::Haiku,
    );

    let mut builder = ContextBuilder::new();
    IntentRouter::add_ambiguity_context(&mut builder, &ambiguous_route);
    let prompt = builder.compose();

    assert!(prompt.contains("ambiguous"));
    assert!(prompt.contains("Combat"));
    assert!(prompt.contains("Dialogue"));
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

    assert!(!prompt.contains("ambiguous"));
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn empty_input_classifies_without_panic() {
    let classifier = MockClassifier::high_confidence(Intent::Exploration);
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_with_classifier("", &ctx, &classifier);
    assert_eq!(route.intent(), Intent::Exploration);
}

#[test]
fn very_long_input_classifies_without_panic() {
    let long_input = "I ".to_string() + &"really ".repeat(1000) + "want to attack";
    let classifier = MockClassifier::high_confidence(Intent::Combat);
    let ctx = TurnContext::default();
    let route = IntentRouter::classify_with_classifier(&long_input, &ctx, &classifier);
    assert_eq!(route.intent(), Intent::Combat);
}

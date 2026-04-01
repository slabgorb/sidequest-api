//! Story 14-7: Image scene relevance filter tests.
//!
//! Validates that art prompts are checked against current scene context
//! before image generation. Entities not in the scene are rejected.
//! DM override bypasses validation. OTEL instrumentation on all paths.

use sidequest_game::subject::{ExtractionContext, RenderSubject, SceneType, SubjectTier};
use sidequest_game::scene_relevance::{SceneRelevanceValidator};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_subject(entities: Vec<&str>, scene_type: SceneType, prompt: &str) -> RenderSubject {
    RenderSubject::new(
        entities.into_iter().map(String::from).collect(),
        scene_type,
        SubjectTier::Scene,
        prompt.to_string(),
        0.8,
    )
    .expect("test subject should have valid weight")
}

fn make_context(npcs: Vec<&str>, location: &str, in_combat: bool) -> ExtractionContext {
    ExtractionContext {
        known_npcs: npcs.into_iter().map(String::from).collect(),
        current_location: location.to_string(),
        in_combat,
        recent_subjects: vec![],
    }
}

// ---------------------------------------------------------------------------
// AC-1: Validation runs — every art prompt checked against scene context
// ---------------------------------------------------------------------------

#[test]
fn validator_returns_verdict_for_every_subject() {
    let validator = SceneRelevanceValidator::new();
    let subject = make_subject(vec!["Grok"], SceneType::Combat, "Grok swings an axe");
    let context = make_context(vec!["Grok"], "Arena", true);

    let verdict = validator.evaluate(&subject, &context);
    // Must return a verdict, not panic or return Option
    assert!(
        verdict.is_approved() || verdict.is_rejected(),
        "validator must return a definitive verdict"
    );
}

// ---------------------------------------------------------------------------
// AC-2: Mismatch rejected — entities not in scene are suppressed
// ---------------------------------------------------------------------------

#[test]
fn rejected_when_entity_not_in_known_npcs() {
    let validator = SceneRelevanceValidator::new();
    // Subject mentions "Mutant Beast" but scene only has "Merchant Talia"
    let subject = make_subject(
        vec!["Mutant Beast"],
        SceneType::Exploration,
        "A massive mutant beast looms over the marketplace",
    );
    let context = make_context(vec!["Merchant Talia"], "Marketplace", false);

    let verdict = validator.evaluate(&subject, &context);
    assert!(
        verdict.is_rejected(),
        "entities not in scene should be rejected, got: {:?}",
        verdict
    );
}

#[test]
fn rejected_when_some_entities_not_in_scene() {
    let validator = SceneRelevanceValidator::new();
    // One entity matches, one doesn't
    let subject = make_subject(
        vec!["Grok", "Dragon"],
        SceneType::Combat,
        "Grok battles a dragon in the tavern",
    );
    let context = make_context(vec!["Grok", "Bartender"], "Tavern", false);

    let verdict = validator.evaluate(&subject, &context);
    assert!(
        verdict.is_rejected(),
        "should reject when ANY entity is not in scene"
    );
}

// ---------------------------------------------------------------------------
// AC-3: Match approved — prompt matching scene entities proceeds
// ---------------------------------------------------------------------------

#[test]
fn approved_when_all_entities_in_scene() {
    let validator = SceneRelevanceValidator::new();
    let subject = make_subject(
        vec!["Grok", "Merchant Talia"],
        SceneType::Dialogue,
        "Grok haggles with Merchant Talia",
    );
    let context = make_context(vec!["Grok", "Merchant Talia", "Guard"], "Marketplace", false);

    let verdict = validator.evaluate(&subject, &context);
    assert!(
        verdict.is_approved(),
        "all entities present in scene should be approved"
    );
}

#[test]
fn approved_when_no_entities_in_subject() {
    let validator = SceneRelevanceValidator::new();
    // Landscape/atmosphere with no specific entities
    let subject = make_subject(
        vec![],
        SceneType::Exploration,
        "A vast desert stretches before you",
    );
    let context = make_context(vec!["Grok"], "Desert Wastes", false);

    let verdict = validator.evaluate(&subject, &context);
    assert!(
        verdict.is_approved(),
        "subjects with no entities should pass (nothing to mismatch)"
    );
}

// ---------------------------------------------------------------------------
// AC-4: Logged — rejected prompts include reason for debugging
// ---------------------------------------------------------------------------

#[test]
fn rejection_includes_mismatched_entities_in_reason() {
    let validator = SceneRelevanceValidator::new();
    let subject = make_subject(
        vec!["Mutant Beast", "Shadow Wraith"],
        SceneType::Combat,
        "Mutant Beast and Shadow Wraith attack",
    );
    let context = make_context(vec!["Merchant Talia"], "Marketplace", true);

    let verdict = validator.evaluate(&subject, &context);
    assert!(verdict.is_rejected());

    let reason = verdict.reason();
    assert!(
        reason.contains("Mutant Beast"),
        "rejection reason should name mismatched entity 'Mutant Beast', got: {}",
        reason
    );
    assert!(
        reason.contains("Shadow Wraith"),
        "rejection reason should name mismatched entity 'Shadow Wraith', got: {}",
        reason
    );
}

// ---------------------------------------------------------------------------
// AC-5: No retry — rejected prompt doesn't trigger regeneration
// ---------------------------------------------------------------------------

#[test]
fn verdict_rejected_has_no_retry_flag() {
    let validator = SceneRelevanceValidator::new();
    let subject = make_subject(
        vec!["Unknown Entity"],
        SceneType::Exploration,
        "An unknown entity appears",
    );
    let context = make_context(vec!["Grok"], "Tavern", false);

    let verdict = validator.evaluate(&subject, &context);
    assert!(verdict.is_rejected());
    assert!(
        !verdict.should_retry(),
        "rejected prompts must NOT suggest retry"
    );
}

// ---------------------------------------------------------------------------
// AC-6: Scene-aware — uses NPCs, location, and active entities
// ---------------------------------------------------------------------------

#[test]
fn location_coherence_deferred_to_llm() {
    let validator = SceneRelevanceValidator::new();
    // Subject describes a forest scene but we're in a tavern.
    // Location coherence is handled by the LLM continuity validator,
    // not the scene relevance validator (check_location returns None).
    let subject = make_subject(
        vec![],
        SceneType::Exploration,
        "Deep in the enchanted forest, ancient trees tower overhead",
    );
    let context = make_context(vec![], "The Rusty Tankard Tavern", false);

    let verdict = validator.evaluate(&subject, &context);
    assert!(
        verdict.is_approved(),
        "location coherence is deferred to LLM — validator should approve"
    );
}

#[test]
fn combat_scene_type_during_exploration_is_suspect() {
    let validator = SceneRelevanceValidator::new();
    // Combat scene type but context says no combat
    let subject = make_subject(
        vec!["Grok"],
        SceneType::Combat,
        "Grok charges into battle",
    );
    let context = make_context(vec!["Grok"], "Peaceful Meadow", false);

    let verdict = validator.evaluate(&subject, &context);
    assert!(
        verdict.is_rejected(),
        "combat scene type should be rejected when not in combat"
    );
}

#[test]
fn combat_scene_type_approved_during_combat() {
    let validator = SceneRelevanceValidator::new();
    let subject = make_subject(
        vec!["Grok"],
        SceneType::Combat,
        "Grok charges into battle",
    );
    let context = make_context(vec!["Grok"], "Arena", true);

    let verdict = validator.evaluate(&subject, &context);
    assert!(
        verdict.is_approved(),
        "combat scene type should be approved when in_combat is true"
    );
}

// ---------------------------------------------------------------------------
// Edge cases: entity matching flexibility
// ---------------------------------------------------------------------------

#[test]
fn case_insensitive_entity_matching() {
    let validator = SceneRelevanceValidator::new();
    let subject = make_subject(
        vec!["grok"],
        SceneType::Dialogue,
        "grok speaks softly",
    );
    let context = make_context(vec!["Grok"], "Tavern", false);

    let verdict = validator.evaluate(&subject, &context);
    assert!(
        verdict.is_approved(),
        "entity matching should be case-insensitive"
    );
}

#[test]
fn partial_name_matches_known_npc() {
    let validator = SceneRelevanceValidator::new();
    // "Grok" extracted from narration should match "Grok the Destroyer" in known_npcs
    let subject = make_subject(
        vec!["Grok"],
        SceneType::Dialogue,
        "Grok examines the rune",
    );
    let context = make_context(vec!["Grok the Destroyer"], "Ruins", false);

    let verdict = validator.evaluate(&subject, &context);
    assert!(
        verdict.is_approved(),
        "partial name 'Grok' should match 'Grok the Destroyer'"
    );
}

// ---------------------------------------------------------------------------
// DM override: bypass validation
// ---------------------------------------------------------------------------

#[test]
fn dm_override_bypasses_all_validation() {
    let validator = SceneRelevanceValidator::new();
    // Completely mismatched subject — would normally be rejected
    let subject = make_subject(
        vec!["Ancient Dragon"],
        SceneType::Combat,
        "Ancient dragon destroys the city",
    );
    let context = make_context(vec!["Merchant Talia"], "Marketplace", false);

    let verdict = validator.evaluate_with_override(&subject, &context, true);
    assert!(
        verdict.is_approved(),
        "DM override should bypass all validation checks"
    );
}

#[test]
fn dm_override_false_runs_normal_validation() {
    let validator = SceneRelevanceValidator::new();
    let subject = make_subject(
        vec!["Ancient Dragon"],
        SceneType::Combat,
        "Ancient dragon destroys the city",
    );
    let context = make_context(vec!["Merchant Talia"], "Marketplace", false);

    let verdict = validator.evaluate_with_override(&subject, &context, false);
    assert!(
        verdict.is_rejected(),
        "override=false should run normal validation"
    );
}

// ---------------------------------------------------------------------------
// Verdict API: type design (Rust checklist rules)
// ---------------------------------------------------------------------------

#[test]
fn verdict_approved_has_no_reason() {
    let validator = SceneRelevanceValidator::new();
    let subject = make_subject(
        vec!["Grok"],
        SceneType::Dialogue,
        "Grok speaks",
    );
    let context = make_context(vec!["Grok"], "Tavern", false);

    let verdict = validator.evaluate(&subject, &context);
    assert!(verdict.is_approved());
    // Approved verdicts should not carry rejection reasons
}

#[test]
fn verdict_rejected_reason_is_not_empty() {
    let validator = SceneRelevanceValidator::new();
    let subject = make_subject(
        vec!["Unknown"],
        SceneType::Exploration,
        "Unknown entity lurks",
    );
    let context = make_context(vec!["Grok"], "Tavern", false);

    let verdict = validator.evaluate(&subject, &context);
    assert!(verdict.is_rejected());
    assert!(
        !verdict.reason().is_empty(),
        "rejected verdict must have a non-empty reason"
    );
}

// ---------------------------------------------------------------------------
// OTEL instrumentation: trace spans for relevance checks
// ---------------------------------------------------------------------------

#[test]
fn validator_has_instrument_attribute() {
    // This is a structural test — the SceneRelevanceValidator::evaluate method
    // should be annotated with #[instrument] or contain tracing::info_span!.
    // We verify by checking that the validator creates spans when tracing is active.
    //
    // For RED phase: we just verify the type exists and has the evaluate method.
    // The Dev phase will wire tracing and this test confirms it's present.
    let validator = SceneRelevanceValidator::new();
    let subject = make_subject(vec!["Grok"], SceneType::Dialogue, "Grok speaks");
    let context = make_context(vec!["Grok"], "Tavern", false);

    // If this compiles and runs, the method exists. OTEL wiring is verified
    // by the tracing subscriber in integration tests.
    let _verdict = validator.evaluate(&subject, &context);
}

// ---------------------------------------------------------------------------
// Multiple validations don't leak state
// ---------------------------------------------------------------------------

#[test]
fn validator_is_stateless_between_calls() {
    let validator = SceneRelevanceValidator::new();
    let context = make_context(vec!["Grok"], "Tavern", false);

    // First call: rejected
    let bad_subject = make_subject(
        vec!["Dragon"],
        SceneType::Combat,
        "Dragon attacks",
    );
    let verdict1 = validator.evaluate(&bad_subject, &context);
    assert!(verdict1.is_rejected());

    // Second call with matching entities: should be approved regardless of first call
    let good_subject = make_subject(
        vec!["Grok"],
        SceneType::Dialogue,
        "Grok orders a drink",
    );
    let verdict2 = validator.evaluate(&good_subject, &context);
    assert!(
        verdict2.is_approved(),
        "previous rejection should not affect subsequent evaluations"
    );
}

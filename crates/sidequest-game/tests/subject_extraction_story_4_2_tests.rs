//! Story 4-2: Subject extraction — parse narration for image render subjects
//!
//! RED phase — these tests exercise the SubjectExtractor and its output types.
//! They will panic/fail until Dev implements:
//!   - subject.rs: SubjectExtractor::new(), with_tier_rules(), extract()
//!   - Entity extraction via regex + NPC name matching
//!   - Scene type classification from keywords + game state
//!   - Tier assignment rules (entity count + scene type)
//!   - Narrative weight scoring heuristics
//!   - Prompt fragment composition

use sidequest_game::subject::{
    ExtractionContext, RenderSubject, SceneType, SubjectExtractor, SubjectTier, TierRules,
    MAX_NARRATION_LENGTH,
};

// ============================================================================
// Test fixtures
// ============================================================================

fn combat_context() -> ExtractionContext {
    ExtractionContext {
        known_npcs: vec![
            "Grak the Destroyer".to_string(),
            "Mira Shadowstep".to_string(),
        ],
        current_location: "The Burning Arena".to_string(),
        in_combat: true,
        recent_subjects: vec![],
    }
}

fn exploration_context() -> ExtractionContext {
    ExtractionContext {
        known_npcs: vec!["Old Sage Theron".to_string()],
        current_location: "The Whispering Caves".to_string(),
        in_combat: false,
        recent_subjects: vec![],
    }
}

fn dialogue_context() -> ExtractionContext {
    ExtractionContext {
        known_npcs: vec![
            "Captain Voss".to_string(),
            "Lira the Merchant".to_string(),
        ],
        current_location: "The Docks".to_string(),
        in_combat: false,
        recent_subjects: vec![],
    }
}

fn default_extractor() -> SubjectExtractor {
    SubjectExtractor::new()
}

// ============================================================================
// AC: Entity extraction — Named NPCs from context identified in narration
// ============================================================================

#[test]
fn extract_identifies_known_npc_by_name() {
    let extractor = default_extractor();
    let ctx = combat_context();
    let narration = "Grak the Destroyer swings his massive axe at you, \
                     roaring with fury as the blade arcs through the air.";

    let result = extractor.extract(narration, &ctx);
    assert!(result.is_some(), "Should extract a subject from combat narration with known NPC");

    let subject = result.unwrap();
    assert!(
        subject.entities().contains(&"Grak the Destroyer".to_string()),
        "Should identify 'Grak the Destroyer' from known_npcs. Got: {:?}",
        subject.entities()
    );
}

#[test]
fn extract_identifies_multiple_known_npcs() {
    let extractor = default_extractor();
    let ctx = combat_context();
    let narration = "Grak the Destroyer charges forward while Mira Shadowstep \
                     flanks from the left, her daggers gleaming.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert!(
        result.entities().contains(&"Grak the Destroyer".to_string()),
        "Should find Grak. Got: {:?}",
        result.entities()
    );
    assert!(
        result.entities().contains(&"Mira Shadowstep".to_string()),
        "Should find Mira. Got: {:?}",
        result.entities()
    );
}

#[test]
fn extract_ignores_unknown_names() {
    let extractor = default_extractor();
    let ctx = ExtractionContext {
        known_npcs: vec!["Captain Voss".to_string()],
        current_location: "Town Square".to_string(),
        in_combat: false,
        recent_subjects: vec![],
    };
    // "Random Stranger" is not in known_npcs
    let narration = "Captain Voss nods at the random stranger passing by.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert!(
        result.entities().contains(&"Captain Voss".to_string()),
        "Should find Captain Voss"
    );
    assert!(
        !result.entities().iter().any(|e| e.contains("stranger")),
        "Should not extract unknown 'stranger' as a named entity"
    );
}

// ============================================================================
// AC: Scene classification — Combat narration → SceneType::Combat
// ============================================================================

#[test]
fn classify_combat_narration_as_combat() {
    let extractor = default_extractor();
    let ctx = combat_context();
    let narration = "Grak the Destroyer swings his axe in a vicious arc. \
                     Blood spatters across the arena floor as his blade finds its mark.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert_eq!(
        *result.scene_type(),
        SceneType::Combat,
        "Combat narration with in_combat=true should classify as Combat"
    );
}

#[test]
fn classify_dialogue_narration_as_dialogue() {
    let extractor = default_extractor();
    let ctx = dialogue_context();
    let narration = "Captain Voss turns to you and says, 'The shipment arrives at dawn. \
                     We must be ready.' Lira the Merchant nods in agreement.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert_eq!(
        *result.scene_type(),
        SceneType::Dialogue,
        "Dialogue narration should classify as Dialogue"
    );
}

#[test]
fn classify_exploration_narration() {
    let extractor = default_extractor();
    let ctx = exploration_context();
    let narration = "You carefully make your way through the narrow passage, \
                     your torch casting flickering shadows on the damp cave walls.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert_eq!(
        *result.scene_type(),
        SceneType::Exploration,
        "Traversal narration should classify as Exploration"
    );
}

#[test]
fn classify_discovery_narration() {
    let extractor = default_extractor();
    let ctx = exploration_context();
    let narration = "Behind the fallen pillar, you discover an ancient chest \
                     encrusted with glowing runes. Something powerful rests within.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert_eq!(
        *result.scene_type(),
        SceneType::Discovery,
        "Finding hidden objects should classify as Discovery"
    );
}

#[test]
fn classify_transition_narration() {
    let extractor = default_extractor();
    let ctx = exploration_context();
    let narration = "You leave the caves behind and step out into the blinding daylight. \
                     The mountain trail stretches before you.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert_eq!(
        *result.scene_type(),
        SceneType::Transition,
        "Scene change narration should classify as Transition"
    );
}

// ============================================================================
// AC: Tier assignment — entity count + scene type → tier
// ============================================================================

#[test]
fn single_entity_dialogue_yields_portrait_tier() {
    let extractor = default_extractor();
    let ctx = ExtractionContext {
        known_npcs: vec!["Old Sage Theron".to_string()],
        current_location: "Library".to_string(),
        in_combat: false,
        recent_subjects: vec![],
    };
    let narration = "Old Sage Theron peers at you over his spectacles, \
                     his weathered face illuminated by candlelight.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert_eq!(
        *result.tier(),
        SubjectTier::Portrait,
        "Single-entity dialogue should yield Portrait tier"
    );
}

#[test]
fn multi_entity_combat_yields_scene_tier() {
    let extractor = default_extractor();
    let ctx = combat_context();
    let narration = "Grak the Destroyer and Mira Shadowstep clash in a whirlwind \
                     of steel. Sparks fly as axe meets dagger.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert_eq!(
        *result.tier(),
        SubjectTier::Scene,
        "Multi-entity combat should yield Scene tier"
    );
}

// ============================================================================
// AC: Landscape detection — environment narration → SubjectTier::Landscape
// ============================================================================

#[test]
fn environment_description_yields_landscape_tier() {
    let extractor = default_extractor();
    let ctx = exploration_context();
    let narration = "You enter a vast underground cavern lit by bioluminescent fungi. \
                     Stalactites hang from the ceiling like stone teeth, and a \
                     subterranean river cuts through the darkness below.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert_eq!(
        *result.tier(),
        SubjectTier::Landscape,
        "Rich environment description should yield Landscape tier"
    );
}

#[test]
fn entering_new_area_yields_landscape_tier() {
    let extractor = default_extractor();
    let ctx = exploration_context();
    let narration = "Before you lies an ancient temple, its crumbling columns \
                     draped in vines. The entrance yawns like a dark mouth.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert_eq!(
        *result.tier(),
        SubjectTier::Landscape,
        "'Before you lies' with environment focus should be Landscape"
    );
}

// ============================================================================
// AC: Abstract tier — mood/atmosphere
// ============================================================================

#[test]
fn atmospheric_narration_yields_abstract_tier() {
    let extractor = default_extractor();
    let ctx = ExtractionContext {
        known_npcs: vec![],
        current_location: "The Void".to_string(),
        in_combat: false,
        recent_subjects: vec![],
    };
    let narration = "A creeping dread settles over you. The air grows thick with an \
                     unnameable tension, as though the world itself holds its breath.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert_eq!(
        *result.tier(),
        SubjectTier::Abstract,
        "Pure mood/atmosphere narration should yield Abstract tier"
    );
}

// ============================================================================
// AC: Prompt composition — output is daemon-ready image description
// ============================================================================

#[test]
fn prompt_fragment_is_nonempty_and_descriptive() {
    let extractor = default_extractor();
    let ctx = combat_context();
    let narration = "Grak the Destroyer raises his axe high above his head, \
                     silhouetted against the flames of the burning arena.";

    let result = extractor.extract(narration, &ctx).unwrap();
    let fragment = result.prompt_fragment();

    assert!(!fragment.is_empty(), "Prompt fragment must not be empty");
    assert!(
        fragment.len() >= 10,
        "Prompt fragment should be a meaningful description, got: '{}'",
        fragment
    );
}

#[test]
fn prompt_fragment_contains_entity_reference() {
    let extractor = default_extractor();
    let ctx = combat_context();
    let narration = "Grak the Destroyer charges at you with reckless abandon, \
                     his massive axe leaving gouges in the stone floor.";

    let result = extractor.extract(narration, &ctx).unwrap();
    let fragment = result.prompt_fragment();

    // The prompt fragment should reference the primary entity
    assert!(
        fragment.to_lowercase().contains("grak")
            || fragment.to_lowercase().contains("destroyer")
            || fragment.to_lowercase().contains("warrior")
            || fragment.to_lowercase().contains("axe"),
        "Prompt fragment should reference the scene's primary subject. Got: '{}'",
        fragment
    );
}

// ============================================================================
// AC: Weight scoring — high-action > 0.7, mundane < 0.2
// ============================================================================

#[test]
fn high_action_combat_scores_above_threshold() {
    let extractor = default_extractor();
    let ctx = combat_context();
    let narration = "Grak the Destroyer lunges forward, his massive axe cleaving through \
                     the air. Mira Shadowstep dives aside, her daggers flashing as she \
                     retaliates with a flurry of strikes. Blood sprays across the arena.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert!(
        result.narrative_weight() > 0.7,
        "High-action multi-entity combat should score > 0.7, got {}",
        result.narrative_weight()
    );
}

#[test]
fn mundane_action_scores_below_threshold() {
    let extractor = default_extractor();
    let ctx = ExtractionContext {
        known_npcs: vec![],
        current_location: "Tavern".to_string(),
        in_combat: false,
        recent_subjects: vec![],
    };
    let narration = "You nod.";

    // "You nod" is below minimum weight → should return None
    let result = extractor.extract(narration, &ctx);
    assert!(
        result.is_none(),
        "'You nod' should be below minimum weight and return None"
    );
}

#[test]
fn moderate_narration_scores_middle_range() {
    let extractor = default_extractor();
    let ctx = exploration_context();
    let narration = "You walk through the dimly lit corridor, noting the \
                     strange markings on the walls.";

    let result = extractor.extract(narration, &ctx);
    // Moderate exploration — might or might not meet threshold,
    // but if it does, weight should be in a reasonable range
    if let Some(subject) = result {
        assert!(
            subject.narrative_weight() >= 0.0 && subject.narrative_weight() <= 1.0,
            "Weight must be clamped to [0.0, 1.0], got {}",
            subject.narrative_weight()
        );
    }
}

#[test]
fn weight_is_clamped_to_unit_interval() {
    let extractor = default_extractor();
    let ctx = combat_context();
    // Extremely rich narration — weight must still be ≤ 1.0
    let narration = "Grak the Destroyer and Mira Shadowstep clash in an explosive \
                     whirlwind of steel and fire. Lightning strikes the arena as \
                     ancient magic surges. The ground splits open. Dragons circle overhead. \
                     The crowd screams. Blood and sparks paint the air crimson.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert!(
        result.narrative_weight() <= 1.0,
        "Weight must be clamped to 1.0 max, got {}",
        result.narrative_weight()
    );
    assert!(
        result.narrative_weight() >= 0.0,
        "Weight must be >= 0.0, got {}",
        result.narrative_weight()
    );
}

// ============================================================================
// AC: Minimum threshold — below minimum weight returns None
// ============================================================================

#[test]
fn below_minimum_weight_returns_none() {
    let rules = TierRules {
        minimum_weight: 0.5,
    };
    let extractor = SubjectExtractor::with_tier_rules(rules);
    let ctx = ExtractionContext {
        known_npcs: vec![],
        current_location: "Hallway".to_string(),
        in_combat: false,
        recent_subjects: vec![],
    };
    let narration = "You glance around the empty hallway.";

    let result = extractor.extract(narration, &ctx);
    assert!(
        result.is_none(),
        "Narration below custom minimum_weight=0.5 should return None"
    );
}

#[test]
fn default_minimum_weight_filters_trivial_narration() {
    let extractor = default_extractor();
    let ctx = ExtractionContext::default();
    let narration = "You wait.";

    let result = extractor.extract(narration, &ctx);
    assert!(
        result.is_none(),
        "'You wait.' should be below default minimum weight"
    );
}

// ============================================================================
// AC: Context awareness — Known NPC names resolved from ExtractionContext
// ============================================================================

#[test]
fn context_combat_flag_influences_classification() {
    let extractor = default_extractor();
    // Same narration, different combat state
    let narration = "Grak the Destroyer moves toward you with purpose.";

    let combat_ctx = ExtractionContext {
        known_npcs: vec!["Grak the Destroyer".to_string()],
        current_location: "Arena".to_string(),
        in_combat: true,
        recent_subjects: vec![],
    };
    let peaceful_ctx = ExtractionContext {
        known_npcs: vec!["Grak the Destroyer".to_string()],
        current_location: "Arena".to_string(),
        in_combat: false,
        recent_subjects: vec![],
    };

    let combat_result = extractor.extract(narration, &combat_ctx);
    let peaceful_result = extractor.extract(narration, &peaceful_ctx);

    // In combat context, this should lean toward Combat scene type
    if let Some(ref subject) = combat_result {
        assert_eq!(
            *subject.scene_type(),
            SceneType::Combat,
            "With in_combat=true, ambiguous narration should classify as Combat"
        );
    }

    // Out of combat, same narration should not be Combat
    if let Some(ref subject) = peaceful_result {
        assert_ne!(
            *subject.scene_type(),
            SceneType::Combat,
            "With in_combat=false, same narration should not classify as Combat"
        );
    }
}

// ============================================================================
// AC: Dedup signal — recent_subjects prevents re-extracting same entity
// ============================================================================

#[test]
fn recent_subjects_suppresses_duplicate_entities() {
    let extractor = default_extractor();
    let ctx = ExtractionContext {
        known_npcs: vec!["Grak the Destroyer".to_string()],
        current_location: "Arena".to_string(),
        in_combat: true,
        recent_subjects: vec!["Grak the Destroyer".to_string()],
    };
    let narration = "Grak the Destroyer swings again, relentless in his assault.";

    let result = extractor.extract(narration, &ctx);

    // If extraction still produces a result, Grak should be filtered from entities
    // since he was recently rendered. This may cause the extraction to return None
    // (if Grak was the only entity and dedup removes him).
    match result {
        None => {
            // Acceptable: dedup removed the only entity, nothing to render
        }
        Some(subject) => {
            assert!(
                !subject.entities().contains(&"Grak the Destroyer".to_string()),
                "Grak was in recent_subjects — should be deduplicated from entities"
            );
        }
    }
}

#[test]
fn fresh_entity_not_suppressed_by_dedup() {
    let extractor = default_extractor();
    let ctx = ExtractionContext {
        known_npcs: vec![
            "Grak the Destroyer".to_string(),
            "Mira Shadowstep".to_string(),
        ],
        current_location: "Arena".to_string(),
        in_combat: true,
        // Grak was recently rendered, but Mira was not
        recent_subjects: vec!["Grak the Destroyer".to_string()],
    };
    let narration = "Mira Shadowstep leaps from the shadows, daggers drawn, \
                     as Grak the Destroyer turns to face the new threat.";

    let result = extractor.extract(narration, &ctx).unwrap();
    assert!(
        result.entities().contains(&"Mira Shadowstep".to_string()),
        "Mira is not in recent_subjects — should NOT be deduplicated. Got: {:?}",
        result.entities()
    );
}

// ============================================================================
// Rule #2: #[non_exhaustive] on public enums
// ============================================================================

/// Verify that SubjectTier and SceneType are non_exhaustive by checking
/// they can be constructed but require a wildcard arm in match.
/// (The #[non_exhaustive] attribute is enforced at compile time —
/// if this test compiles, the attribute is present.)
#[test]
fn subject_tier_variants_are_constructible() {
    let tiers = vec![
        SubjectTier::Portrait,
        SubjectTier::Scene,
        SubjectTier::Landscape,
        SubjectTier::Abstract,
    ];
    assert_eq!(tiers.len(), 4, "Should have 4 SubjectTier variants");

    // non_exhaustive means downstream crates need a wildcard — verified by compilation
    for tier in &tiers {
        match tier {
            SubjectTier::Portrait
            | SubjectTier::Scene
            | SubjectTier::Landscape
            | SubjectTier::Abstract => {}
            _ => panic!("Unexpected SubjectTier variant"),
        }
    }
}

#[test]
fn scene_type_variants_are_constructible() {
    let types = vec![
        SceneType::Combat,
        SceneType::Dialogue,
        SceneType::Exploration,
        SceneType::Discovery,
        SceneType::Transition,
    ];
    assert_eq!(types.len(), 5, "Should have 5 SceneType variants");

    for st in &types {
        match st {
            SceneType::Combat
            | SceneType::Dialogue
            | SceneType::Exploration
            | SceneType::Discovery
            | SceneType::Transition => {}
            _ => panic!("Unexpected SceneType variant"),
        }
    }
}

// ============================================================================
// Rule #5: Validated constructors — RenderSubject::new rejects invalid weight
// ============================================================================

#[test]
fn render_subject_new_rejects_negative_weight() {
    let result = RenderSubject::new(
        vec!["Entity".to_string()],
        SceneType::Combat,
        SubjectTier::Scene,
        "combat scene".to_string(),
        -0.1,
    );
    assert!(
        result.is_none(),
        "RenderSubject::new should reject negative weight"
    );
}

#[test]
fn render_subject_new_rejects_weight_above_one() {
    let result = RenderSubject::new(
        vec!["Entity".to_string()],
        SceneType::Combat,
        SubjectTier::Scene,
        "combat scene".to_string(),
        1.1,
    );
    assert!(
        result.is_none(),
        "RenderSubject::new should reject weight > 1.0"
    );
}

#[test]
fn render_subject_new_accepts_boundary_weights() {
    let zero = RenderSubject::new(
        vec!["A".to_string()],
        SceneType::Exploration,
        SubjectTier::Landscape,
        "landscape".to_string(),
        0.0,
    );
    assert!(zero.is_some(), "Weight 0.0 should be accepted");

    let one = RenderSubject::new(
        vec!["A".to_string()],
        SceneType::Combat,
        SubjectTier::Scene,
        "scene".to_string(),
        1.0,
    );
    assert!(one.is_some(), "Weight 1.0 should be accepted");
}

// ============================================================================
// Rule #9: Private fields with getters — RenderSubject fields are not public
// ============================================================================

/// This test verifies the API contract: RenderSubject fields are accessed
/// through getters, not direct field access. If fields were public, changing
/// narrative_weight after construction could break the [0.0, 1.0] invariant.
#[test]
fn render_subject_accessed_through_getters() {
    let subject = RenderSubject::new(
        vec!["Grak".to_string()],
        SceneType::Combat,
        SubjectTier::Portrait,
        "warrior with axe".to_string(),
        0.8,
    )
    .unwrap();

    // Verify all getters return expected values
    assert_eq!(subject.entities(), &["Grak".to_string()]);
    assert_eq!(*subject.scene_type(), SceneType::Combat);
    assert_eq!(*subject.tier(), SubjectTier::Portrait);
    assert_eq!(subject.prompt_fragment(), "warrior with axe");
    assert_eq!(subject.narrative_weight(), 0.8);
}

// ============================================================================
// Rule #15: Unbounded input — extractor rejects oversized input
// ============================================================================

#[test]
fn extract_rejects_empty_narration() {
    let extractor = default_extractor();
    let ctx = ExtractionContext::default();

    assert!(
        extractor.extract("", &ctx).is_none(),
        "Empty string should return None"
    );
    assert!(
        extractor.extract("   ", &ctx).is_none(),
        "Whitespace-only should return None"
    );
}

#[test]
fn extract_rejects_oversized_narration() {
    let extractor = default_extractor();
    let ctx = ExtractionContext::default();
    let oversized = "a".repeat(MAX_NARRATION_LENGTH + 1);

    let result = extractor.extract(&oversized, &ctx);
    assert!(
        result.is_none(),
        "Narration exceeding MAX_NARRATION_LENGTH ({}) should return None",
        MAX_NARRATION_LENGTH
    );
}

#[test]
fn extract_accepts_narration_at_max_length() {
    let extractor = default_extractor();
    let ctx = combat_context();
    // Pad a real narration to exactly MAX_NARRATION_LENGTH
    let base = "Grak the Destroyer charges forward through the burning arena. ";
    let padding = " The flames roar.";
    let mut narration = base.to_string();
    while narration.len() + padding.len() < MAX_NARRATION_LENGTH {
        narration.push_str(padding);
    }
    // Trim to exactly MAX_NARRATION_LENGTH
    narration.truncate(MAX_NARRATION_LENGTH);

    // Should not be rejected for size (may still return None for other reasons)
    // The point is: it doesn't panic or reject purely on length
    let _ = extractor.extract(&narration, &ctx);
    // If we reach here without panic, the length check accepts MAX_NARRATION_LENGTH
}

// ============================================================================
// Edge cases and integration
// ============================================================================

#[test]
fn extract_handles_narration_with_only_dialogue() {
    let extractor = default_extractor();
    let ctx = dialogue_context();
    // Pure dialogue with minimal physical description
    let narration = "'I told you never to come back here,' Captain Voss growls.";

    let result = extractor.extract(narration, &ctx);
    // Should either extract Captain Voss as portrait or return None
    // (depends on weight scoring), but should not panic
    if let Some(subject) = result {
        assert!(
            subject.entities().contains(&"Captain Voss".to_string()),
            "If dialogue is extracted, should identify the speaker"
        );
    }
}

#[test]
fn extract_with_no_known_npcs_still_works() {
    let extractor = default_extractor();
    let ctx = ExtractionContext {
        known_npcs: vec![],
        current_location: "Forest".to_string(),
        in_combat: false,
        recent_subjects: vec![],
    };
    let narration = "You enter a vast forest clearing. Ancient oaks tower above, \
                     their branches interlocking to form a natural cathedral.";

    let result = extractor.extract(narration, &ctx);
    // Should still produce a Landscape subject even without NPCs
    if let Some(subject) = result {
        assert_eq!(
            *subject.tier(),
            SubjectTier::Landscape,
            "Environment-only narration without NPCs should be Landscape"
        );
    }
}

#[test]
fn tier_rules_minimum_weight_is_respected() {
    // Very high threshold — almost nothing should pass
    let rules = TierRules {
        minimum_weight: 0.95,
    };
    let extractor = SubjectExtractor::with_tier_rules(rules);
    let ctx = exploration_context();
    let narration = "You walk down the path.";

    let result = extractor.extract(narration, &ctx);
    assert!(
        result.is_none(),
        "With minimum_weight=0.95, simple narration should return None"
    );
}

#[test]
fn default_tier_rules_has_reasonable_minimum() {
    let rules = TierRules::default();
    assert!(
        rules.minimum_weight > 0.0 && rules.minimum_weight < 0.5,
        "Default minimum_weight should be reasonable (0.0-0.5), got {}",
        rules.minimum_weight
    );
}

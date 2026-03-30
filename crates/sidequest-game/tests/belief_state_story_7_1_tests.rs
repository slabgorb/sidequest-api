//! Story 7-1: BeliefState model — per-NPC knowledge bubbles
//!
//! RED phase — these tests reference types and methods that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - BeliefState struct — per-NPC knowledge container
//!   - Belief enum — Fact / Suspicion / Claim with associated data
//!   - BeliefSource — how the NPC acquired the belief (Witnessed, ToldBy, Inferred, Overheard)
//!   - Credibility — per-source trust score (f32 clamped to 0.0..=1.0)
//!   - BeliefState::add_belief() — insert with dedup by subject
//!   - BeliefState::beliefs_about() — query by subject string
//!   - BeliefState::credibility_of() — get trust score for a source NPC
//!   - BeliefState::update_credibility() — adjust trust after verification
//!   - Serde round-trip for all types
//!   - Integration with Npc struct (npc.belief_state field)
//!
//! ACs tested: Model definition, Belief variants, Source tracking,
//!             Credibility scoring, Query interface, Persistence,
//!             NPC integration, Edge cases

use sidequest_game::belief_state::{
    Belief, BeliefSource, BeliefState, Credibility,
};

// ============================================================================
// AC: BeliefState model — container for per-NPC knowledge
// ============================================================================

#[test]
fn belief_state_new_is_empty() {
    let state = BeliefState::new();
    assert!(state.beliefs().is_empty(), "new BeliefState should have no beliefs");
    assert!(state.credibility_scores().is_empty(), "new BeliefState should have no credibility scores");
}

#[test]
fn belief_state_default_matches_new() {
    let from_new = BeliefState::new();
    let from_default = BeliefState::default();
    assert_eq!(from_new.beliefs().len(), from_default.beliefs().len());
}

// ============================================================================
// AC: Belief variants — Fact, Suspicion, Claim with content and metadata
// ============================================================================

#[test]
fn belief_fact_variant() {
    let belief = Belief::Fact {
        subject: "murder weapon".to_string(),
        content: "The dagger was found in the library".to_string(),
        turn_learned: 5,
        source: BeliefSource::Witnessed,
    };
    assert!(matches!(belief, Belief::Fact { .. }));
    assert_eq!(belief.subject(), "murder weapon");
    assert_eq!(belief.content(), "The dagger was found in the library");
    assert_eq!(belief.turn_learned(), 5);
}

#[test]
fn belief_suspicion_variant() {
    let belief = Belief::Suspicion {
        subject: "butler".to_string(),
        content: "The butler was near the library that night".to_string(),
        turn_learned: 7,
        source: BeliefSource::Inferred,
        confidence: 0.6,
    };
    assert!(matches!(belief, Belief::Suspicion { .. }));
    assert_eq!(belief.subject(), "butler");
    if let Belief::Suspicion { confidence, .. } = &belief {
        assert!(*confidence >= 0.0 && *confidence <= 1.0);
    }
}

#[test]
fn belief_claim_variant() {
    let belief = Belief::Claim {
        subject: "alibi".to_string(),
        content: "The cook says she was in the kitchen all evening".to_string(),
        turn_learned: 3,
        source: BeliefSource::ToldBy("Cook".to_string()),
        believed: false,
    };
    assert!(matches!(belief, Belief::Claim { .. }));
    if let Belief::Claim { believed, .. } = &belief {
        assert!(!believed);
    }
}

#[test]
fn belief_suspicion_confidence_clamped_high() {
    let belief = Belief::Suspicion {
        subject: "test".to_string(),
        content: "over-confident".to_string(),
        turn_learned: 1,
        source: BeliefSource::Inferred,
        confidence: 1.5, // should clamp to 1.0
    };
    if let Belief::Suspicion { confidence, .. } = &belief {
        assert!(
            *confidence <= 1.0,
            "suspicion confidence should be clamped to 1.0, got {}",
            confidence
        );
    }
}

#[test]
fn belief_suspicion_confidence_clamped_low() {
    let belief = Belief::Suspicion {
        subject: "test".to_string(),
        content: "negative confidence".to_string(),
        turn_learned: 1,
        source: BeliefSource::Inferred,
        confidence: -0.3, // should clamp to 0.0
    };
    if let Belief::Suspicion { confidence, .. } = &belief {
        assert!(
            *confidence >= 0.0,
            "suspicion confidence should be clamped to 0.0, got {}",
            confidence
        );
    }
}

// ============================================================================
// AC: BeliefSource — how the NPC acquired knowledge
// ============================================================================

#[test]
fn belief_source_witnessed() {
    let source = BeliefSource::Witnessed;
    assert!(matches!(source, BeliefSource::Witnessed));
}

#[test]
fn belief_source_told_by_carries_npc_name() {
    let source = BeliefSource::ToldBy("Mayor Blackwood".to_string());
    if let BeliefSource::ToldBy(name) = &source {
        assert_eq!(name, "Mayor Blackwood");
    } else {
        panic!("expected ToldBy variant");
    }
}

#[test]
fn belief_source_inferred() {
    let source = BeliefSource::Inferred;
    assert!(matches!(source, BeliefSource::Inferred));
}

#[test]
fn belief_source_overheard() {
    let source = BeliefSource::Overheard;
    assert!(matches!(source, BeliefSource::Overheard));
}

// ============================================================================
// AC: Credibility — per-source trust scores
// ============================================================================

#[test]
fn credibility_new_clamped_to_unit_range() {
    let cred = Credibility::new(0.8);
    assert_eq!(cred.score(), 0.8);

    let high = Credibility::new(2.0);
    assert!(high.score() <= 1.0, "credibility must clamp to 1.0");

    let low = Credibility::new(-0.5);
    assert!(low.score() >= 0.0, "credibility must clamp to 0.0");
}

#[test]
fn credibility_default_is_neutral() {
    let cred = Credibility::default();
    assert!(
        (cred.score() - 0.5).abs() < f32::EPSILON,
        "default credibility should be 0.5 (neutral), got {}",
        cred.score()
    );
}

#[test]
fn credibility_adjust_increases() {
    let mut cred = Credibility::new(0.5);
    cred.adjust(0.2);
    assert!(
        (cred.score() - 0.7).abs() < 0.001,
        "adjusted credibility should be ~0.7, got {}",
        cred.score()
    );
}

#[test]
fn credibility_adjust_decreases() {
    let mut cred = Credibility::new(0.5);
    cred.adjust(-0.3);
    assert!(
        (cred.score() - 0.2).abs() < 0.001,
        "adjusted credibility should be ~0.2, got {}",
        cred.score()
    );
}

#[test]
fn credibility_adjust_clamps_at_bounds() {
    let mut cred = Credibility::new(0.9);
    cred.adjust(0.5); // would be 1.4
    assert!(cred.score() <= 1.0, "credibility must not exceed 1.0");

    let mut cred2 = Credibility::new(0.1);
    cred2.adjust(-0.5); // would be -0.4
    assert!(cred2.score() >= 0.0, "credibility must not go below 0.0");
}

// ============================================================================
// AC: add_belief — insert beliefs into BeliefState
// ============================================================================

#[test]
fn add_belief_stores_fact() {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Fact {
        subject: "weapon".to_string(),
        content: "The dagger was bloody".to_string(),
        turn_learned: 3,
        source: BeliefSource::Witnessed,
    });
    assert_eq!(state.beliefs().len(), 1);
}

#[test]
fn add_multiple_beliefs_accumulates() {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Fact {
        subject: "weapon".to_string(),
        content: "The dagger was bloody".to_string(),
        turn_learned: 3,
        source: BeliefSource::Witnessed,
    });
    state.add_belief(Belief::Suspicion {
        subject: "butler".to_string(),
        content: "Butler acted nervous".to_string(),
        turn_learned: 5,
        source: BeliefSource::Witnessed,
        confidence: 0.4,
    });
    state.add_belief(Belief::Claim {
        subject: "alibi".to_string(),
        content: "Cook says she was in kitchen".to_string(),
        turn_learned: 6,
        source: BeliefSource::ToldBy("Cook".to_string()),
        believed: true,
    });
    assert_eq!(state.beliefs().len(), 3);
}

// ============================================================================
// AC: beliefs_about — query beliefs by subject
// ============================================================================

#[test]
fn beliefs_about_returns_matching_beliefs() {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Fact {
        subject: "weapon".to_string(),
        content: "Dagger found in library".to_string(),
        turn_learned: 3,
        source: BeliefSource::Witnessed,
    });
    state.add_belief(Belief::Suspicion {
        subject: "weapon".to_string(),
        content: "Dagger may belong to butler".to_string(),
        turn_learned: 8,
        source: BeliefSource::Inferred,
        confidence: 0.5,
    });
    state.add_belief(Belief::Fact {
        subject: "location".to_string(),
        content: "Body was in the study".to_string(),
        turn_learned: 1,
        source: BeliefSource::Witnessed,
    });

    let weapon_beliefs = state.beliefs_about("weapon");
    assert_eq!(weapon_beliefs.len(), 2, "should find 2 beliefs about 'weapon'");
}

#[test]
fn beliefs_about_returns_empty_for_unknown_subject() {
    let state = BeliefState::new();
    let result = state.beliefs_about("nonexistent");
    assert!(result.is_empty());
}

#[test]
fn beliefs_about_is_case_sensitive() {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Fact {
        subject: "Weapon".to_string(),
        content: "The dagger".to_string(),
        turn_learned: 1,
        source: BeliefSource::Witnessed,
    });
    // Lowercase query should NOT match uppercase subject
    let result = state.beliefs_about("weapon");
    assert!(result.is_empty(), "beliefs_about should be case-sensitive");
}

// ============================================================================
// AC: credibility_of / update_credibility — per-NPC trust tracking
// ============================================================================

#[test]
fn credibility_of_unknown_npc_returns_default() {
    let state = BeliefState::new();
    let cred = state.credibility_of("Unknown NPC");
    assert!(
        (cred.score() - 0.5).abs() < f32::EPSILON,
        "unknown NPC should have default credibility 0.5, got {}",
        cred.score()
    );
}

#[test]
fn update_credibility_stores_and_retrieves() {
    let mut state = BeliefState::new();
    state.update_credibility("Mayor Blackwood", 0.9);
    let cred = state.credibility_of("Mayor Blackwood");
    assert!(
        (cred.score() - 0.9).abs() < 0.001,
        "credibility should be 0.9, got {}",
        cred.score()
    );
}

#[test]
fn update_credibility_overwrites_previous() {
    let mut state = BeliefState::new();
    state.update_credibility("Butler", 0.8);
    state.update_credibility("Butler", 0.3);
    let cred = state.credibility_of("Butler");
    assert!(
        (cred.score() - 0.3).abs() < 0.001,
        "credibility should be updated to 0.3, got {}",
        cred.score()
    );
}

#[test]
fn update_credibility_clamps_to_range() {
    let mut state = BeliefState::new();
    state.update_credibility("Liar", -0.5);
    assert!(state.credibility_of("Liar").score() >= 0.0);

    state.update_credibility("Saint", 1.5);
    assert!(state.credibility_of("Saint").score() <= 1.0);
}

// ============================================================================
// AC: Serde persistence — all types round-trip through JSON
// ============================================================================

#[test]
fn belief_fact_serde_round_trip() {
    let belief = Belief::Fact {
        subject: "weapon".to_string(),
        content: "Bloody dagger".to_string(),
        turn_learned: 3,
        source: BeliefSource::Witnessed,
    };
    let json = serde_json::to_string(&belief).expect("serialize fact belief");
    let restored: Belief = serde_json::from_str(&json).expect("deserialize fact belief");
    assert_eq!(restored.subject(), "weapon");
    assert_eq!(restored.content(), "Bloody dagger");
}

#[test]
fn belief_suspicion_serde_round_trip() {
    let belief = Belief::Suspicion {
        subject: "butler".to_string(),
        content: "Nervous behavior".to_string(),
        turn_learned: 5,
        source: BeliefSource::Inferred,
        confidence: 0.7,
    };
    let json = serde_json::to_string(&belief).expect("serialize suspicion");
    let restored: Belief = serde_json::from_str(&json).expect("deserialize suspicion");
    if let Belief::Suspicion { confidence, .. } = &restored {
        assert!((confidence - 0.7).abs() < 0.001);
    } else {
        panic!("expected Suspicion variant after round-trip");
    }
}

#[test]
fn belief_claim_serde_round_trip() {
    let belief = Belief::Claim {
        subject: "alibi".to_string(),
        content: "Was in the kitchen".to_string(),
        turn_learned: 6,
        source: BeliefSource::ToldBy("Cook".to_string()),
        believed: false,
    };
    let json = serde_json::to_string(&belief).expect("serialize claim");
    let restored: Belief = serde_json::from_str(&json).expect("deserialize claim");
    if let Belief::Claim { believed, source, .. } = &restored {
        assert!(!believed);
        assert!(matches!(source, BeliefSource::ToldBy(name) if name == "Cook"));
    } else {
        panic!("expected Claim variant after round-trip");
    }
}

#[test]
fn belief_source_all_variants_serde_round_trip() {
    let sources = vec![
        BeliefSource::Witnessed,
        BeliefSource::ToldBy("Mayor".to_string()),
        BeliefSource::Inferred,
        BeliefSource::Overheard,
    ];
    for source in sources {
        let json = serde_json::to_string(&source).expect("serialize source");
        let restored: BeliefSource = serde_json::from_str(&json).expect("deserialize source");
        assert_eq!(
            std::mem::discriminant(&source),
            std::mem::discriminant(&restored),
            "source variant should survive round-trip: {}",
            json
        );
    }
}

#[test]
fn credibility_serde_round_trip() {
    let cred = Credibility::new(0.75);
    let json = serde_json::to_string(&cred).expect("serialize credibility");
    let restored: Credibility = serde_json::from_str(&json).expect("deserialize credibility");
    assert!((restored.score() - 0.75).abs() < 0.001);
}

#[test]
fn belief_state_full_serde_round_trip() {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Fact {
        subject: "weapon".to_string(),
        content: "Bloody dagger in library".to_string(),
        turn_learned: 3,
        source: BeliefSource::Witnessed,
    });
    state.add_belief(Belief::Claim {
        subject: "alibi".to_string(),
        content: "Cook claims kitchen".to_string(),
        turn_learned: 6,
        source: BeliefSource::ToldBy("Cook".to_string()),
        believed: true,
    });
    state.update_credibility("Cook", 0.8);

    let json = serde_json::to_string(&state).expect("serialize belief state");
    let restored: BeliefState = serde_json::from_str(&json).expect("deserialize belief state");

    assert_eq!(restored.beliefs().len(), 2);
    assert!((restored.credibility_of("Cook").score() - 0.8).abs() < 0.001);
}

// ============================================================================
// AC: NPC integration — belief_state field on Npc
// ============================================================================

#[test]
fn npc_has_belief_state_field() {
    // This test verifies that Npc struct has a belief_state: BeliefState field.
    // Import will fail until the field is added to npc.rs
    use sidequest_game::npc::Npc;

    let json = r#"{
        "name": "Mayor Blackwood",
        "description": "A portly gentleman with nervous eyes",
        "personality": "Calculating",
        "level": 5,
        "hp": 30,
        "max_hp": 30,
        "ac": 12,
        "inventory": {"items": [], "gold": 100},
        "statuses": [],
        "disposition": 0,
        "pronouns": "he/him",
        "appearance": "Tall with grey hair",
        "role": "Mayor",
        "location": "Town Hall"
    }"#;
    let npc: Npc = serde_json::from_str(json).expect("deserialize NPC without belief_state");
    // NPCs without belief_state field should get empty default (backward compat)
    assert!(
        npc.belief_state.beliefs().is_empty(),
        "NPC deserialized without belief_state should have empty beliefs"
    );
}

#[test]
fn npc_belief_state_persists_through_serde() {
    use sidequest_game::npc::Npc;

    // Build a minimal NPC with beliefs, serialize, deserialize, verify beliefs survive
    let npc_json = r#"{
        "name": "Mayor Blackwood",
        "description": "A portly gentleman",
        "personality": "Calculating",
        "level": 5,
        "hp": 30,
        "max_hp": 30,
        "ac": 12,
        "inventory": {"items": [], "gold": 100},
        "statuses": [],
        "disposition": 0,
        "pronouns": "he/him",
        "appearance": "Tall",
        "role": "Mayor",
        "location": "Town Hall",
        "belief_state": {
            "beliefs": [
                {
                    "Fact": {
                        "subject": "murder",
                        "content": "The victim was poisoned",
                        "turn_learned": 1,
                        "source": "Witnessed"
                    }
                }
            ],
            "credibility_scores": {}
        }
    }"#;
    let npc: Npc = serde_json::from_str(npc_json).expect("deserialize NPC with beliefs");
    assert_eq!(npc.belief_state.beliefs().len(), 1);
    assert_eq!(npc.belief_state.beliefs()[0].subject(), "murder");

    // Round-trip
    let json = serde_json::to_string(&npc).expect("serialize NPC with beliefs");
    let restored: Npc = serde_json::from_str(&json).expect("round-trip NPC");
    assert_eq!(restored.belief_state.beliefs().len(), 1);
}

// ============================================================================
// Edge cases — the paranoid cases
// ============================================================================

#[test]
fn empty_subject_belief_still_stores() {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Fact {
        subject: "".to_string(),
        content: "Something with no subject".to_string(),
        turn_learned: 1,
        source: BeliefSource::Witnessed,
    });
    assert_eq!(state.beliefs().len(), 1);
    let found = state.beliefs_about("");
    assert_eq!(found.len(), 1);
}

#[test]
fn beliefs_about_with_many_subjects_filters_correctly() {
    let mut state = BeliefState::new();
    for i in 0..20 {
        state.add_belief(Belief::Fact {
            subject: format!("subject_{}", i % 5),
            content: format!("fact {}", i),
            turn_learned: i as u64,
            source: BeliefSource::Witnessed,
        });
    }
    // 20 beliefs, 5 distinct subjects, 4 each
    let result = state.beliefs_about("subject_0");
    assert_eq!(result.len(), 4, "should find 4 beliefs for subject_0");
}

#[test]
fn credibility_scores_independent_per_npc() {
    let mut state = BeliefState::new();
    state.update_credibility("Alice", 0.9);
    state.update_credibility("Bob", 0.1);

    assert!((state.credibility_of("Alice").score() - 0.9).abs() < 0.001);
    assert!((state.credibility_of("Bob").score() - 0.1).abs() < 0.001);
    // Unrelated NPC unaffected
    assert!((state.credibility_of("Charlie").score() - 0.5).abs() < f32::EPSILON);
}

#[test]
fn belief_turn_learned_is_preserved() {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Fact {
        subject: "timeline".to_string(),
        content: "First event".to_string(),
        turn_learned: 1,
        source: BeliefSource::Witnessed,
    });
    state.add_belief(Belief::Fact {
        subject: "timeline".to_string(),
        content: "Later event".to_string(),
        turn_learned: 100,
        source: BeliefSource::Witnessed,
    });

    let timeline = state.beliefs_about("timeline");
    assert_eq!(timeline[0].turn_learned(), 1);
    assert_eq!(timeline[1].turn_learned(), 100);
}

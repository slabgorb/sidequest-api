//! Story 15-10: Wire seed_lore_from_char_creation — RED phase tests
//!
//! The function `seed_lore_from_char_creation` exists in sidequest-game (lore.rs:315),
//! is exported, and has extensive unit tests. BUT it is never called from production
//! server code during character creation.
//!
//! These tests assert the behavior we WANT:
//!   1. After character creation completes, the lore store contains CharacterCreation-sourced entries
//!   2. The server's dispatch_character_creation function actually calls seed_lore_from_char_creation
//!
//! ACs covered:
//!   AC-1: seed_lore_from_char_creation is called during character creation
//!   AC-2: Lore store contains CharacterCreation entries after chargen completes

use std::collections::HashMap;

use sidequest_game::{seed_lore_from_char_creation, LoreCategory, LoreSource, LoreStore};
use sidequest_genre::{CharCreationChoice, CharCreationScene, MechanicalEffects};

// ============================================================================
// Helper: build realistic CharCreationScene data
// ============================================================================

fn test_mechanical_effects() -> MechanicalEffects {
    MechanicalEffects {
        class_hint: None,
        race_hint: None,
        mutation_hint: None,
        item_hint: None,
        affinity_hint: None,
        training_hint: None,
        background: None,
        personality_trait: None,
        emotional_state: None,
        relationship: None,
        goals: None,
        allows_freeform: None,
        rig_type_hint: None,
        rig_trait: None,
        catch_phrase: None,
        stat_bonuses: HashMap::new(),
        pronoun_hint: None,
    }
}

fn test_char_creation_scenes() -> Vec<CharCreationScene> {
    vec![
        CharCreationScene {
            id: "origin".to_string(),
            title: "Where do you come from?".to_string(),
            narration: "The wasteland stretches before you...".to_string(),
            choices: vec![
                CharCreationChoice {
                    label: "Vault Dweller".to_string(),
                    description: "Raised in an underground vault, you know little of the surface world".to_string(),
                    mechanical_effects: test_mechanical_effects(),
                },
                CharCreationChoice {
                    label: "Wastelander".to_string(),
                    description: "Born under open skies, you've survived by wit and grit".to_string(),
                    mechanical_effects: test_mechanical_effects(),
                },
            ],
            allows_freeform: None,
            hook_prompt: None,
        },
        CharCreationScene {
            id: "motivation".to_string(),
            title: "What drives you?".to_string(),
            narration: "Every wanderer has a reason...".to_string(),
            choices: vec![
                CharCreationChoice {
                    label: "Revenge".to_string(),
                    description: "Someone took everything from you. You'll take it back.".to_string(),
                    mechanical_effects: test_mechanical_effects(),
                },
                CharCreationChoice {
                    label: "Curiosity".to_string(),
                    description: "The old world left secrets. You intend to find them.".to_string(),
                    mechanical_effects: test_mechanical_effects(),
                },
            ],
            allows_freeform: None,
            hook_prompt: None,
        },
    ]
}

// ============================================================================
// AC-1: seed_lore_from_char_creation produces CharacterCreation lore entries
//
// This test proves the function itself works. It passes.
// The REAL gap is that the server never calls it.
// ============================================================================

#[test]
fn seed_lore_from_char_creation_populates_store() {
    let mut store = LoreStore::new();
    let scenes = test_char_creation_scenes();

    let count = seed_lore_from_char_creation(&mut store, &scenes);

    // 2 scenes × 2 choices each = 4 fragments
    assert_eq!(count, 4);
    assert_eq!(store.len(), 4);

    // All should be Character category
    let char_frags = store.query_by_category(&LoreCategory::Character);
    assert_eq!(char_frags.len(), 4);

    // All should have CharacterCreation source
    for frag in &char_frags {
        assert_eq!(
            frag.source(),
            &LoreSource::CharacterCreation,
            "Fragment '{}' should have CharacterCreation source",
            frag.id()
        );
    }
}

// ============================================================================
// AC-2: WIRING TEST — dispatch_character_creation calls seed_lore_from_char_creation
//
// This is the failing test (RED). The server's dispatch_character_creation()
// function currently does NOT call seed_lore_from_char_creation(). This test
// reads the source and verifies the call exists within the confirmation branch.
//
// Per CLAUDE.md: "Every Test Suite Needs a Wiring Test" — unit tests prove the
// component works in isolation, but this test proves it's actually connected.
// ============================================================================

#[test]
fn dispatch_character_creation_calls_seed_lore_from_char_creation() {
    let source = include_str!("../src/lib.rs");

    // Find the dispatch_character_creation function body
    let fn_start = source
        .find("async fn dispatch_character_creation(")
        .expect("dispatch_character_creation function should exist in server lib.rs");

    // Extract a generous slice of the function (it's ~200 lines)
    let fn_body = &source[fn_start..std::cmp::min(fn_start + 12_000, source.len())];

    assert!(
        fn_body.contains("seed_lore_from_char_creation"),
        "dispatch_character_creation() must call seed_lore_from_char_creation() \
         to wire character backstory into the lore store. Currently it does not — \
         character creation scenes are lost after chargen completes."
    );
}

// ============================================================================
// AC-2 (supplementary): Lore store should have CharacterCreation entries
// after the "confirmation" phase completes.
//
// This verifies the wiring from a different angle: the confirmation branch
// (where character is built and session transitions to Playing) must seed lore
// BEFORE the builder is set to None, because the scenes are owned by the builder.
// ============================================================================

#[test]
fn confirmation_branch_seeds_lore_before_builder_cleared() {
    let source = include_str!("../src/lib.rs");

    let fn_start = source
        .find("async fn dispatch_character_creation(")
        .expect("dispatch_character_creation should exist");

    let fn_body = &source[fn_start..std::cmp::min(fn_start + 12_000, source.len())];

    // Find where builder is set to None
    let builder_none_pos = fn_body.find("*builder = None");
    assert!(
        builder_none_pos.is_some(),
        "dispatch_character_creation should clear the builder"
    );

    // seed_lore_from_char_creation must appear BEFORE *builder = None,
    // because the builder owns the scenes data.
    let seed_pos = fn_body.find("seed_lore_from_char_creation");
    assert!(
        seed_pos.is_some(),
        "seed_lore_from_char_creation must be called in dispatch_character_creation"
    );
    assert!(
        seed_pos.unwrap() < builder_none_pos.unwrap(),
        "seed_lore_from_char_creation must be called BEFORE builder is cleared (set to None), \
         because the builder owns the CharCreationScene data needed for lore seeding"
    );
}

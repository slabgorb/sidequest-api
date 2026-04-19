//! Story 31-4: Wire hp_formula evaluation with CON modifier after stat rolling
//!
//! RED phase — these tests verify that CharacterBuilder.build() evaluates the
//! hp_formula from the genre pack's RulesConfig using rolled stats.
//!
//! Currently, build() ignores hp_formula entirely and just uses class_hp_bases
//! lookup with a hardcoded fallback to 10. These tests will fail until Dev
//! wires hp_formula evaluation into the build pipeline.
//!
//! ACs tested:
//!   1. hp_formula from genre pack is loaded and evaluated
//!   2. CON modifier is correctly extracted from rolled stats
//!   3. Character HP is set during build()
//!   4. Multiple formula patterns work (not just C&C)
//!   5. OTEL span emitted for HP calculation

use std::collections::HashMap;

use sidequest_game::builder::{BuilderError, CharacterBuilder};
use sidequest_genre::{CharCreationChoice, CharCreationScene, MechanicalEffects, RulesConfig};

// ============================================================================
// Test fixtures (reusing 31-1 patterns)
// ============================================================================

/// C&C-style scenes: the_roll (stat gen), pronouns, the_mouth.
fn caverns_scenes() -> Vec<CharCreationScene> {
    vec![
        CharCreationScene {
            id: "the_roll".to_string(),
            title: "3d6. In Order.".to_string(),
            narration: "The man with no fingers pushes six bone dice across the wood.".to_string(),
            choices: vec![],
            allows_freeform: Some(false),
            hook_prompt: None,
            loading_text: None,
            mechanical_effects: Some(MechanicalEffects {
                stat_generation: Some("roll_3d6_strict".to_string()),
                ..MechanicalEffects::default()
            }),
        },
        CharCreationScene {
            id: "pronouns".to_string(),
            title: "Who Are You?".to_string(),
            narration: "For the tally.".to_string(),
            choices: vec![CharCreationChoice {
                label: "he/him".to_string(),
                description: "He.".to_string(),
                mechanical_effects: MechanicalEffects {
                    pronoun_hint: Some("he/him".to_string()),
                    ..MechanicalEffects::default()
                },
            }],
            allows_freeform: Some(false),
            hook_prompt: None,
            loading_text: None,
            mechanical_effects: None,
        },
        CharCreationScene {
            id: "the_mouth".to_string(),
            title: "The Dungeon Waits".to_string(),
            narration: "You have a torch, ten feet of rope, and no backstory.".to_string(),
            choices: vec![],
            allows_freeform: Some(false),
            hook_prompt: None,
            loading_text: None,
            mechanical_effects: None,
        },
    ]
}

/// C&C rules with hp_formula: "8 + CON_modifier"
fn rules_with_hp_formula() -> RulesConfig {
    RulesConfig {
        tone: "gritty".to_string(),
        lethality: "high".to_string(),
        magic_level: "none".to_string(),
        stat_generation: "roll_3d6_strict".to_string(),
        point_buy_budget: 0,
        ability_score_names: vec![
            "STR".to_string(),
            "DEX".to_string(),
            "CON".to_string(),
            "INT".to_string(),
            "WIS".to_string(),
            "CHA".to_string(),
        ],
        allowed_classes: vec!["Delver".to_string()],
        allowed_races: vec!["Human".to_string()],
        class_hp_bases: HashMap::from([("Delver".to_string(), 8)]),
        default_class: Some("Delver".to_string()),
        default_race: Some("Human".to_string()),
        default_hp: Some(8),
        default_ac: Some(10),
        default_location: Some("The mouth of the dungeon".to_string()),
        default_time_of_day: Some("dawn".to_string()),
        hp_formula: Some("8 + CON_modifier".to_string()),
        banned_spells: vec![],
        custom_rules: HashMap::new(),
        stat_display_fields: vec![],
        encounter_base_tension: HashMap::new(),
        race_label: None,
        class_label: None,
        confrontations: vec![],
        resources: vec![],
        xp_affinity: None,
        initiative_rules: std::collections::HashMap::new(),
    }
}

/// Rules with NO hp_formula — should fall back to class_hp_bases.
fn rules_without_hp_formula() -> RulesConfig {
    RulesConfig {
        hp_formula: None,
        ..rules_with_hp_formula()
    }
}

/// Drive builder through all scenes to Confirmation, then build.
fn build_character(rules: &RulesConfig) -> Result<sidequest_game::Character, BuilderError> {
    let scenes = caverns_scenes();
    let mut builder = CharacterBuilder::new(scenes, rules, None);

    // Scene 0: the_roll — auto-advance (no choices)
    builder.apply_freeform("")?;
    // Scene 1: pronouns — pick choice 0
    builder.apply_choice(0)?;
    // Scene 2: the_mouth — auto-advance
    builder.apply_freeform("")?;

    assert!(
        builder.is_confirmation(),
        "Should be in Confirmation after all scenes"
    );
    builder.build("Grist the Ratcatcher")
}

// ============================================================================
// AC-1: hp_formula from genre pack is loaded and evaluated
// ============================================================================

#[test]
fn hp_formula_is_evaluated_not_just_class_base() {
    // With hp_formula "8 + CON_modifier", HP should NOT always be exactly 8
    // (the class_hp_bases value). It should vary based on rolled CON.
    // Run multiple builds — with random 3d6 stats, the chance of CON always
    // being exactly 10 (modifier 0) across 10 builds is negligible.
    let rules = rules_with_hp_formula();
    let mut hp_values: Vec<i32> = Vec::new();

    for _ in 0..10 {
        let character = build_character(&rules).expect("build should succeed");
        hp_values.push(character.core.edge.current);
    }

    // If hp_formula were evaluated, HP should vary (8 + CON_modifier).
    // With random CON from 3-18, modifiers range from -4 to +4.
    // HP range should be 4-12. Not all values should be 8.
    let all_same = hp_values.iter().all(|&hp| hp == 8);
    assert!(
        !all_same,
        "HP is always 8 across 10 builds — hp_formula is not being evaluated. \
         Got: {:?}. Expected variation from CON modifier.",
        hp_values
    );
}

// ============================================================================
// AC-2: CON modifier is correctly extracted from rolled stats
// ============================================================================

#[test]
#[ignore = "Story 39-3: hp_formula results now seed edge.base_max via YAML (not wired yet). Epic 39 discards base_hp in builder.rs pending 39-3."]
fn hp_reflects_con_modifier() {
    // Build a character and verify HP = 8 + floor((CON - 10) / 2)
    let rules = rules_with_hp_formula();
    let character = build_character(&rules).expect("build should succeed");

    let con_value = *character.stats.get("CON").expect("CON stat should exist");
    let con_modifier = (con_value - 10) / 2; // D&D-style floor division
    let expected_hp = 8 + con_modifier;

    assert_eq!(
        character.core.edge.current, expected_hp,
        "HP should be 8 + CON_modifier. CON={}, modifier={}, expected HP={}, got HP={}",
        con_value, con_modifier, expected_hp, character.core.edge.current
    );
}

#[test]
fn max_hp_matches_hp() {
    // At level 1, hp and max_hp should be the same.
    let rules = rules_with_hp_formula();
    let character = build_character(&rules).expect("build should succeed");

    assert_eq!(
        character.core.edge.current, character.core.edge.max,
        "At level 1, hp ({}) and max_hp ({}) should be equal",
        character.core.edge.current, character.core.edge.max
    );
}

// ============================================================================
// AC-3: Fallback when no hp_formula is set
// ============================================================================

#[test]
#[ignore = "Story 39-3: class_hp_bases fallback now seeds edge.base_max via YAML (not wired yet)."]
fn no_hp_formula_falls_back_to_class_hp_bases() {
    // Without hp_formula, should use class_hp_bases lookup (Delver=8).
    let rules = rules_without_hp_formula();
    let character = build_character(&rules).expect("build should succeed");

    assert_eq!(
        character.core.edge.current, 8,
        "Without hp_formula, HP should fall back to class_hp_bases (Delver=8), got {}",
        character.core.edge.current
    );
}

// ============================================================================
// AC-4: HP is always >= 1 (no zero or negative HP from bad CON)
// ============================================================================

#[test]
fn hp_minimum_is_one() {
    // Even with CON 3 (modifier -4), HP = 8 + (-4) = 4, which is fine.
    // But the system should enforce a floor of 1 regardless of formula result.
    // Run enough builds that we'd see very low CON values.
    let rules = rules_with_hp_formula();

    for _ in 0..20 {
        let character = build_character(&rules).expect("build should succeed");
        assert!(
            character.core.edge.current >= 1,
            "HP should never be less than 1, got {} (CON = {:?})",
            character.core.edge.current,
            character.stats.get("CON")
        );
    }
}

// ============================================================================
// Wiring test: CharacterBuilder stores and uses hp_formula from RulesConfig
// ============================================================================

#[test]
fn builder_accepts_hp_formula_from_rules_config() {
    // Verify that CharacterBuilder can be constructed with an hp_formula
    // and that the formula affects the build output.
    let rules = rules_with_hp_formula();
    assert!(
        rules.hp_formula.is_some(),
        "Test fixture should have hp_formula set"
    );

    // Build should succeed — this is the wiring test.
    let character = build_character(&rules).expect("build should succeed with hp_formula");

    // HP should be set (non-zero)
    assert!(
        character.core.edge.current > 0,
        "Character HP should be positive, got {}",
        character.core.edge.current
    );
}

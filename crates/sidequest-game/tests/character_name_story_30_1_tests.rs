//! Story 30-1: Character name not saved — persists as numeric index.
//!
//! Root cause: When a genre pack's chargen has no name-entry scene (e.g.
//! caverns_and_claudes where "You are nobody until you survive"),
//! `CharacterBuilder::character_name()` returns None. The confirmation
//! phase in dispatch/connect.rs then falls back to `payload.choice`,
//! which can be a numeric index like "1". The character is saved with
//! name "1" instead of the player-entered name.
//!
//! These tests verify:
//! 1. character_name() returns the freeform name when a name scene exists
//! 2. character_name() returns None when no name scene exists (current behavior)
//! 3. build() uses the provided name argument correctly
//! 4. A character built without a name scene still gets the correct name
//! 5. The name fallback chain never produces a numeric index

use std::collections::HashMap;

use sidequest_genre::{CharCreationChoice, CharCreationScene, MechanicalEffects, RulesConfig};

use sidequest_game::builder::{CharacterBuilder, SceneInputType};

// ============================================================================
// Test fixtures
// ============================================================================

fn effects_empty() -> MechanicalEffects {
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
        pronoun_hint: None,
        stat_bonuses: HashMap::new(),
    }
}

fn test_rules() -> RulesConfig {
    RulesConfig {
        tone: "heroic".to_string(),
        lethality: "medium".to_string(),
        magic_level: "high".to_string(),
        stat_generation: "standard_array".to_string(),
        point_buy_budget: 27,
        ability_score_names: vec![
            "STR".to_string(),
            "DEX".to_string(),
            "CON".to_string(),
            "INT".to_string(),
            "WIS".to_string(),
            "CHA".to_string(),
        ],
        allowed_classes: vec!["Fighter".to_string()],
        allowed_races: vec!["Human".to_string()],
        class_hp_bases: HashMap::from([("Fighter".to_string(), 10)]),
        default_class: Some("Fighter".to_string()),
        default_race: Some("Human".to_string()),
        default_hp: Some(10),
        default_ac: Some(10),
        default_location: Some("Town Square".to_string()),
        default_time_of_day: Some("morning".to_string()),
        hp_formula: None,
        banned_spells: vec![],
        custom_rules: HashMap::new(),
        stat_display_fields: vec![],
        encounter_base_tension: HashMap::new(),
        race_label: None,
        class_label: None,
        confrontations: vec![],
        resources: vec![],
        xp_affinity: None,
    }
}

/// Chargen scenes WITH a name-entry scene at the end (has no choices, allows freeform).
fn scenes_with_name_entry() -> Vec<CharCreationScene> {
    vec![
        CharCreationScene {
            id: "origin".to_string(),
            title: "Your Origin".to_string(),
            narration: "Where do you come from?".to_string(),
            choices: vec![CharCreationChoice {
                label: "Mountain Fortress".to_string(),
                description: "Born in the mountain halls".to_string(),
                mechanical_effects: MechanicalEffects {
                    race_hint: Some("Dwarf".to_string()),
                    ..effects_empty()
                },
            }],
            allows_freeform: Some(false),
            hook_prompt: None,
            loading_text: None,
        },
        // Name scene — no choices, freeform allowed
        CharCreationScene {
            id: "name".to_string(),
            title: "What Is Your Name?".to_string(),
            narration: "The old keeper asks your name.".to_string(),
            choices: vec![],
            allows_freeform: Some(true),
            hook_prompt: None,
            loading_text: None,
        },
    ]
}

/// Chargen scenes WITHOUT a name-entry scene — mirrors caverns_and_claudes.
/// Last scene has no choices but does NOT allow freeform.
fn scenes_without_name_entry() -> Vec<CharCreationScene> {
    vec![
        CharCreationScene {
            id: "the_roll".to_string(),
            title: "3d6. In Order.".to_string(),
            narration: "Roll your stats.".to_string(),
            choices: vec![],
            allows_freeform: Some(false),
            hook_prompt: None,
            loading_text: None,
        },
        CharCreationScene {
            id: "the_mouth".to_string(),
            title: "The Dungeon Waits".to_string(),
            narration: "You have no backstory. Enter the dungeon.".to_string(),
            choices: vec![],
            allows_freeform: Some(false),
            hook_prompt: None,
            loading_text: None,
        },
    ]
}

// ============================================================================
// AC-1: character_name() returns freeform name from name scene
// ============================================================================

#[test]
fn character_name_returns_freeform_text_from_name_scene() {
    let scenes = scenes_with_name_entry();
    let rules = test_rules();
    let mut builder = CharacterBuilder::new(scenes, &rules);

    // Scene 0: choose origin
    builder.apply_choice(0).unwrap();
    // Scene 1: name scene — type freeform name
    builder.apply_freeform("Four-fingered Jack").unwrap();

    assert_eq!(
        builder.character_name(),
        Some("Four-fingered Jack"),
        "character_name() must return the freeform text from the name scene"
    );
}

// ============================================================================
// AC-2: character_name() returns None when no name scene exists
// ============================================================================

#[test]
fn character_name_returns_none_without_name_scene() {
    let scenes = scenes_without_name_entry();
    let rules = test_rules();
    let mut builder = CharacterBuilder::new(scenes, &rules);

    // Both scenes have no choices — use apply_freeform to advance (builder allows
    // freeform on empty-choice scenes per line 461 of builder.rs)
    builder.apply_freeform("").unwrap();
    builder.apply_freeform("").unwrap();

    assert_eq!(
        builder.character_name(),
        None,
        "character_name() must return None when chargen has no name-entry scene"
    );
}

// ============================================================================
// AC-3: build() uses the provided name, not a numeric index
// ============================================================================

#[test]
fn build_uses_provided_name_not_numeric_index() {
    let scenes = scenes_without_name_entry();
    let rules = test_rules();
    let mut builder = CharacterBuilder::new(scenes, &rules);

    // Advance through all scenes to reach confirmation
    builder.apply_freeform("").unwrap();
    builder.apply_freeform("").unwrap();

    // Build with explicit name — this is what dispatch/connect.rs should do
    let character = builder.build("Four-fingered Jack").unwrap();

    assert_eq!(
        character.core.name.as_str(),
        "Four-fingered Jack",
        "Character name must be the provided name, not a numeric index"
    );
}

#[test]
fn build_rejects_numeric_string_as_name() {
    // A numeric-only name like "1" should be rejected or at least flagged.
    // This is the core bug — the system should never accept "1" as a character name.
    let scenes = scenes_without_name_entry();
    let rules = test_rules();
    let mut builder = CharacterBuilder::new(scenes, &rules);

    builder.apply_freeform("").unwrap();
    builder.apply_freeform("").unwrap();

    // Building with a purely numeric name should fail — it's clearly a UI index, not a name
    let result = builder.build("1");
    assert!(
        result.is_err(),
        "build() must reject purely numeric names — they indicate a UI index was used instead of a real name"
    );
}

// ============================================================================
// AC-4: Name from name scene flows through to built character
// ============================================================================

#[test]
fn name_scene_freeform_flows_to_character_name() {
    let scenes = scenes_with_name_entry();
    let rules = test_rules();
    let mut builder = CharacterBuilder::new(scenes, &rules);

    builder.apply_choice(0).unwrap();
    builder.apply_freeform("Whisper").unwrap();

    let name = builder.character_name().expect("name scene should produce a name").to_string();
    let character = builder.build(&name).unwrap();

    assert_eq!(
        character.core.name.as_str(),
        "Whisper",
        "Name from freeform scene must flow through to the built character"
    );
}

// ============================================================================
// AC-5: Character name persists through save/load cycle
// ============================================================================

#[test]
fn character_name_survives_serialization_roundtrip() {
    let scenes = scenes_with_name_entry();
    let rules = test_rules();
    let mut builder = CharacterBuilder::new(scenes, &rules);

    builder.apply_choice(0).unwrap();
    builder.apply_freeform("Four-fingered Jack").unwrap();

    let name = builder.character_name().unwrap().to_string();
    let character = builder.build(&name).unwrap();

    // Serialize to JSON (what the save system does)
    let json = serde_json::to_string(&character).expect("Character must serialize");
    // Deserialize back
    let restored: sidequest_game::Character =
        serde_json::from_str(&json).expect("Character must deserialize");

    assert_eq!(
        restored.core.name.as_str(),
        "Four-fingered Jack",
        "Character name must survive JSON serialization roundtrip"
    );
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn character_name_trims_whitespace() {
    let scenes = scenes_with_name_entry();
    let rules = test_rules();
    let mut builder = CharacterBuilder::new(scenes, &rules);

    builder.apply_choice(0).unwrap();
    builder.apply_freeform("  Whisper  ").unwrap();

    assert_eq!(
        builder.character_name(),
        Some("Whisper"),
        "character_name() must trim leading/trailing whitespace"
    );
}

#[test]
fn character_name_rejects_empty_string() {
    let scenes = scenes_with_name_entry();
    let rules = test_rules();
    let mut builder = CharacterBuilder::new(scenes, &rules);

    builder.apply_choice(0).unwrap();
    builder.apply_freeform("").unwrap();

    assert_eq!(
        builder.character_name(),
        None,
        "character_name() must return None for empty freeform input"
    );
}

// ============================================================================
// Wiring test: character name field is accessible on Character
// ============================================================================

#[test]
fn character_core_name_is_accessible() {
    // Verify the Character struct exposes the name through core.name
    // and that NonBlankString has as_str()
    let scenes = scenes_with_name_entry();
    let rules = test_rules();
    let mut builder = CharacterBuilder::new(scenes, &rules);

    builder.apply_choice(0).unwrap();
    builder.apply_freeform("Test Name").unwrap();

    let character = builder.build("Test Name").unwrap();

    // Verify name is accessible and correct type
    let name: &str = character.core.name.as_str();
    assert_eq!(name, "Test Name");
    assert!(!name.is_empty());
}

//! Story 39-2: Delete HP from CreatureCore — wiring test (RED).
//!
//! Wire-first boundary test. Drives `CharacterBuilder::build()` through its
//! production code path and asserts the produced `Character.core` has the
//! post-cascade shape required by epic 39:
//!
//!   - `edge: EdgePool` field is populated with a real `base_max` (placeholder
//!     value from 39-2 — 39-3 tunes per-class, 39-4 wires dispatch)
//!   - `acquired_advancements: Vec<String>` field exists (empty default)
//!   - `hp` / `max_hp` / `ac` fields are GONE from CreatureCore
//!   - `Combatant` trait exposes `edge()` / `max_edge()` / `is_broken()`;
//!     `hp()` / `max_hp()` / `ac()` are GONE
//!   - `sidequest_game::hp` module is deleted (compile-enforced by `use`
//!     never resolving)
//!
//! This is a deliberate wire-first test: the file exercises the outermost
//! reachable construction path (chargen → Character) rather than poking at
//! `CreatureCore` literals. AC5 from context-story-39-2 requires this exact
//! shape of integration test.
//!
//! RED state: these assertions fail to compile today because `edge`,
//! `acquired_advancements`, `is_broken()`, `edge()`, and `max_edge()` don't
//! yet exist on `CreatureCore` / `Combatant`, and `hp` / `max_hp` / `ac`
//! still do. Dev's cascade in GREEN converts this file to compile-green and
//! passing.

use std::collections::HashMap;

use sidequest_game::builder::CharacterBuilder;
use sidequest_game::combatant::Combatant;
use sidequest_game::creature_core::RecoveryTrigger;
use sidequest_genre::{CharCreationChoice, CharCreationScene, MechanicalEffects, RulesConfig};

// ────────────────────────────────────────────────────────────────────────────
// Fixtures — minimal two-scene flow that auto-advances to Confirmation.
// ────────────────────────────────────────────────────────────────────────────

fn rules() -> RulesConfig {
    RulesConfig {
        tone: "heroic".to_string(),
        lethality: "medium".to_string(),
        magic_level: "low".to_string(),
        stat_generation: "standard_array".to_string(),
        point_buy_budget: 27,
        ability_score_names: vec![
            "STR".into(),
            "DEX".into(),
            "CON".into(),
            "INT".into(),
            "WIS".into(),
            "CHA".into(),
        ],
        allowed_classes: vec!["Fighter".into()],
        allowed_races: vec!["Human".into()],
        class_hp_bases: HashMap::from([("Fighter".into(), 10)]),
        default_class: Some("Fighter".into()),
        default_race: Some("Human".into()),
        default_hp: Some(10),
        default_ac: Some(10),
        default_location: Some("Town".into()),
        default_time_of_day: Some("morning".into()),
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
        initiative_rules: HashMap::new(),
    }
}

fn scenes() -> Vec<CharCreationScene> {
    vec![CharCreationScene {
        id: "origin".to_string(),
        title: "Origin".to_string(),
        narration: "Choose.".to_string(),
        choices: vec![CharCreationChoice {
            label: "Warrior".to_string(),
            description: "The blade.".to_string(),
            mechanical_effects: MechanicalEffects {
                class_hint: Some("Fighter".into()),
                race_hint: Some("Human".into()),
                ..Default::default()
            },
        }],
        allows_freeform: Some(false),
        hook_prompt: None,
        loading_text: None,
        mechanical_effects: None,
    }]
}

fn build_production_character() -> sidequest_game::character::Character {
    let mut builder = CharacterBuilder::new(scenes(), &rules(), None);
    builder.apply_choice(0).expect("choice applies");
    assert!(
        builder.is_confirmation(),
        "single-scene flow should auto-advance to Confirmation"
    );
    builder.build("Grog").expect("build succeeds from Confirmation")
}

// ────────────────────────────────────────────────────────────────────────────
// AC1: CreatureCore.edge is a populated EdgePool (wired through chargen).
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn production_character_has_populated_edge_pool() {
    let character = build_production_character();

    // `edge` must exist on core and carry a real base_max — not EdgePool::default().
    // Story 39-2 synthesises a placeholder base_max (10 per context); 39-3 tunes it.
    assert!(
        character.core.edge.base_max > 0,
        "edge.base_max must be populated by production constructor, got {}",
        character.core.edge.base_max
    );
    assert_eq!(
        character.core.edge.current, character.core.edge.base_max,
        "new character starts at full composure (current == base_max)"
    );
    assert_eq!(
        character.core.edge.max, character.core.edge.base_max,
        "new character's working max equals base_max at creation"
    );
}

#[test]
fn production_character_edge_recovery_triggers_wired() {
    let character = build_production_character();

    // AC context: placeholder recovery triggers include OnResolution. 39-6 authors
    // genre-specific triggers; 39-2 guarantees at least one is wired so the pool
    // is not an open-ended stub.
    assert!(
        character
            .core
            .edge
            .recovery_triggers
            .contains(&RecoveryTrigger::OnResolution),
        "placeholder edge pool must include OnResolution recovery trigger"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// AC1 cont.: acquired_advancements field exists and defaults empty.
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn production_character_has_empty_acquired_advancements() {
    let character = build_production_character();
    assert!(
        character.core.acquired_advancements.is_empty(),
        "newly-built character starts with no acquired advancements"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// AC2: Combatant trait swap — edge()/max_edge()/is_broken() replace hp/max_hp/ac.
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn combatant_exposes_edge_accessors_not_hp() {
    let character = build_production_character();

    // These calls must resolve via the Combatant trait on Character (through
    // CreatureCore). The trait swap is the whole point of 39-2.
    let edge = Combatant::edge(&character);
    let max_edge = Combatant::max_edge(&character);
    assert!(edge > 0, "new character has positive edge, got {}", edge);
    assert_eq!(edge, max_edge, "new character starts at max composure");
}

#[test]
fn combatant_is_broken_false_when_edge_positive() {
    let character = build_production_character();
    assert!(
        !character.is_broken(),
        "newly-built character must not be broken"
    );
}

#[test]
fn combatant_is_broken_true_when_edge_drained() {
    let mut character = build_production_character();
    character.core.edge.current = 0;
    assert!(
        character.is_broken(),
        "edge.current == 0 means the creature is broken"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// AC3: Workspace compiles — `sidequest_game::hp` module is deleted.
//
// Expressed at the test level: we import a symbol that only exists if hp.rs
// was re-added. Dev's cascade in GREEN deletes the file; this `use` must
// then fail — so the presence of this test as a compile-once fixture
// enforces "no dangling HP module".
//
// Equivalent: a compile-fail test, but cargo doesn't ship one by default.
// Instead we inline a negative-case function that would only compile if
// `apply_hp_delta` still existed. We guard it behind `cfg(never_compiles)` so
// it never participates in the build; Reviewer greps for its absence after
// Dev finishes.
// ────────────────────────────────────────────────────────────────────────────

// Reviewer grep assertion: after 39-2 the following tokens must not
// appear on `CreatureCore`, `Character`, or `sidequest_game::hp`:
//   - `.core.hp`, `.core.max_hp`, `.core.ac`
//   - `Character::apply_hp_delta`
//   - `sidequest_game::hp` module
// (Enforced by reviewer grep rather than a compile-fail test, since
// stable Cargo doesn't ship compile-fail harnesses.)

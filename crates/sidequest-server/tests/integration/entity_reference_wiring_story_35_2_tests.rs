//! Story 35-2 RED: Entity reference hot-path validation wiring into dispatch.
//!
//! Tests that the dispatch pipeline (dispatch/mod.rs) calls entity_reference
//! validation after update_npc_registry(), emitting OTEL ValidationWarning
//! events for unresolved entity names — without blocking dispatch.
//!
//! ACs covered:
//!   AC-1: EntityRegistry::from_snapshot(&ctx.snapshot) built after update_npc_registry()
//!   AC-2: extract_potential_references(&clean_narration) called on narration
//!   AC-3: Unresolved refs emit WatcherEventBuilder("entity_reference", ValidationWarning)
//!   AC-4: Dispatch NOT blocked — informational OTEL only
//!   AC-5: Integration: narration with non-existent NPC triggers OTEL warning
//!   AC-6: Wiring: entity_reference has non-test consumer in dispatch/mod.rs

use std::collections::HashMap;

use sidequest_agents::entity_reference::{extract_potential_references, EntityRegistry};
use sidequest_game::{Character, CreatureCore, Disposition, GameSnapshot, Inventory, Item, Npc};
use sidequest_protocol::NonBlankString;

// ===========================================================================
// Test infrastructure: snapshot builders
// ===========================================================================

fn mock_game_snapshot() -> GameSnapshot {
    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        location: "The Rusty Valve".to_string(),
        time_of_day: "dusk".to_string(),
        atmosphere: "tense and electric".to_string(),
        current_region: "flickering_reach".to_string(),
        discovered_regions: vec!["flickering_reach".to_string()],
        ..GameSnapshot::default()
    }
}

fn make_npc(name: &str) -> Npc {
    Npc {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test NPC").unwrap(),
            personality: NonBlankString::new("Stoic").unwrap(),
            level: 3,
            hp: 20,
            max_hp: 20,
            ac: 12,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        voice_id: None,
        disposition: Disposition::new(0),
        pronouns: None,
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        location: Some(NonBlankString::new("The Rusty Valve").unwrap()),
        ocean: None,
        belief_state: sidequest_game::belief_state::BeliefState::default(),
        resolution_tier: sidequest_game::npc::ResolutionTier::default(),
        non_transactional_interactions: 0,
        jungian_id: None,
        rpg_role_id: None,
        npc_role_id: None,
        resolved_archetype: None,
    }
}

fn make_character(name: &str, item_names: Vec<&str>) -> Character {
    let items: Vec<Item> = item_names
        .into_iter()
        .map(|iname| Item {
            id: NonBlankString::new(&iname.to_lowercase().replace(' ', "_")).unwrap(),
            name: NonBlankString::new(iname).unwrap(),
            description: NonBlankString::new("A test item").unwrap(),
            category: NonBlankString::new("weapon").unwrap(),
            value: 10,
            weight: 1.0,
            rarity: NonBlankString::new("common").unwrap(),
            narrative_weight: 0.5,
            tags: vec![],
            equipped: false,
            quantity: 1,
            uses_remaining: None,
            state: sidequest_game::ItemState::Carried,
        })
        .collect();

    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test character").unwrap(),
            personality: NonBlankString::new("Brave").unwrap(),
            level: 5,
            hp: 30,
            max_hp: 30,
            ac: 14,
            xp: 0,
            inventory: Inventory { items, gold: 100 },
            statuses: vec![],
        },
        backstory: NonBlankString::new("A hero on a quest").unwrap(),
        narrative_state: "exploring".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: String::new(),
        stats: HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
            resolved_archetype: None,
    }
}

// ===========================================================================
// AC-6: Wiring test — entity_reference has a non-test consumer in dispatch
// ===========================================================================

#[test]
fn dispatch_pipeline_uses_entity_reference_module() {
    let source = include_str!("../../src/dispatch/mod.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);
    assert!(
        production_code.contains("entity_reference")
            || production_code.contains("EntityRegistry")
            || production_code.contains("extract_potential_references"),
        "dispatch/mod.rs must import or use entity_reference module in production code \
         (not just tests) — story 35-2 AC-6"
    );
}

// ===========================================================================
// AC-1: EntityRegistry::from_snapshot called in dispatch after NPC registry
// ===========================================================================

#[test]
fn dispatch_pipeline_builds_entity_registry_from_snapshot() {
    let source = include_str!("../../src/dispatch/mod.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);
    assert!(
        production_code.contains("EntityRegistry::from_snapshot"),
        "dispatch/mod.rs must call EntityRegistry::from_snapshot(&ctx.snapshot) \
         after update_npc_registry() — story 35-2 AC-1"
    );
}

// ===========================================================================
// AC-2: extract_potential_references called on clean_narration
// ===========================================================================

#[test]
fn dispatch_pipeline_calls_extract_potential_references() {
    let source = include_str!("../../src/dispatch/mod.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);
    assert!(
        production_code.contains("extract_potential_references"),
        "dispatch/mod.rs must call extract_potential_references(&clean_narration) \
         on the current narration — story 35-2 AC-2"
    );
}

// ===========================================================================
// AC-3: WatcherEventBuilder("entity_reference", ValidationWarning) emitted
// ===========================================================================

#[test]
fn dispatch_pipeline_emits_entity_reference_validation_warning() {
    let source = include_str!("../../src/dispatch/mod.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);

    // Must use WatcherEventBuilder with "entity_reference" component
    assert!(
        production_code.contains("\"entity_reference\"")
            && production_code.contains("ValidationWarning"),
        "dispatch/mod.rs must emit WatcherEventBuilder::new(\"entity_reference\", \
         WatcherEventType::ValidationWarning) for unresolved references — story 35-2 AC-3"
    );
}

// ===========================================================================
// AC-4: Dispatch not blocked — informational OTEL only
// ===========================================================================

#[test]
fn entity_reference_check_does_not_block_dispatch() {
    let source = include_str!("../../src/dispatch/mod.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);

    // First verify the entity_reference block exists at all (prerequisite)
    assert!(
        production_code.contains("entity_reference"),
        "Prerequisite: entity_reference must appear in dispatch/mod.rs production code"
    );

    // Find the entity_reference section and verify it doesn't use `?` or `return`
    // that would abort dispatch. The check should be fire-and-forget OTEL.
    if let Some(pos) = production_code.find("entity_reference") {
        // Grab a window around the entity_reference usage (the block should be ~20 lines)
        let window_end = (pos + 500_usize).min(production_code.len());
        let window = &production_code[pos..window_end];

        // The block must not contain early returns or error propagation
        let has_question_mark = window.contains("?;") || window.contains("? ;");
        let has_early_return = window.contains("return Err") || window.contains("return None");

        assert!(
            !has_question_mark && !has_early_return,
            "entity_reference validation must not block dispatch — \
             no `?` or `return Err/None` in the validation block. \
             This is informational OTEL only — story 35-2 AC-4"
        );
    }
}

// ===========================================================================
// AC-1 ordering: entity_reference runs AFTER update_npc_registry
// ===========================================================================

#[test]
fn entity_reference_check_runs_after_update_npc_registry() {
    let source = include_str!("../../src/dispatch/mod.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);

    let npc_registry_pos = production_code.find("update_npc_registry(");
    let entity_ref_pos = production_code
        .find("EntityRegistry::from_snapshot")
        .or_else(|| production_code.find("extract_potential_references"))
        .or_else(|| production_code.find("entity_reference"));

    assert!(
        npc_registry_pos.is_some(),
        "Prerequisite: update_npc_registry() must exist in dispatch/mod.rs"
    );
    assert!(
        entity_ref_pos.is_some(),
        "entity_reference validation must exist in dispatch/mod.rs — story 35-2 AC-1"
    );

    let npc_pos = npc_registry_pos.unwrap();
    let ent_pos = entity_ref_pos.unwrap();
    assert!(
        ent_pos > npc_pos,
        "entity_reference validation (at byte {}) must appear AFTER \
         update_npc_registry() (at byte {}) in dispatch pipeline — story 35-2 AC-1",
        ent_pos,
        npc_pos
    );
}

// ===========================================================================
// AC-5: Integration — narration with unknown NPC produces unresolved refs
//
// This tests the contract: given a snapshot with known NPCs and narration
// mentioning an unknown NPC, EntityRegistry + extract_potential_references
// correctly identifies unresolved references. The dispatch wiring (AC 1-3)
// ensures this logic actually runs in the hot path.
// ===========================================================================

#[test]
fn narration_with_unknown_npc_produces_unresolved_references() {
    // Snapshot has only "Grimjaw" as a known NPC
    let mut snapshot = mock_game_snapshot();
    snapshot.npcs = vec![make_npc("Grimjaw")];
    snapshot.characters = vec![make_character("Kael", vec!["Rusty Blade"])];

    let registry = EntityRegistry::from_snapshot(&snapshot);

    // Narration mentions "Mordecai" who is NOT in the snapshot
    let narration = "The crowd parts as Grimjaw nods to Kael. \
                     Without warning, Mordecai steps from the shadows.";
    let references = extract_potential_references(narration);

    // Filter to only unresolved references
    let unresolved: Vec<&String> = references.iter().filter(|r| !registry.matches(r)).collect();

    assert!(
        !unresolved.is_empty(),
        "Narration mentioning unknown NPC 'Mordecai' should produce at least one \
         unresolved reference; extracted refs: {:?}, unresolved: {:?}",
        references,
        unresolved
    );
    assert!(
        unresolved.iter().any(|r| r.contains("Mordecai")),
        "Unresolved references should include 'Mordecai'; got: {:?}",
        unresolved
    );
}

#[test]
fn narration_with_only_known_npcs_produces_no_unresolved_references() {
    let mut snapshot = mock_game_snapshot();
    snapshot.npcs = vec![make_npc("Grimjaw")];
    snapshot.characters = vec![make_character("Kael", vec![])];

    let registry = EntityRegistry::from_snapshot(&snapshot);

    // Narration mentions only known entities
    let narration = "The tavern quiets as Grimjaw slams his fist on the table. \
                     Kael reaches for his weapon.";
    let references = extract_potential_references(narration);

    let unresolved: Vec<&String> = references.iter().filter(|r| !registry.matches(r)).collect();

    assert!(
        unresolved.is_empty(),
        "Narration referencing only known entities should produce zero unresolved \
         references; got: {:?}",
        unresolved
    );
}

#[test]
fn multiple_unknown_npcs_each_produce_unresolved_references() {
    let mut snapshot = mock_game_snapshot();
    snapshot.characters = vec![make_character("Kael", vec![])];
    // No NPCs in snapshot — everyone mentioned is unknown

    let registry = EntityRegistry::from_snapshot(&snapshot);

    let narration = "The battle rages as Mordecai and Zephira clash in the courtyard. \
                     Kael watches from the shadows.";
    let references = extract_potential_references(narration);

    let unresolved: Vec<&String> = references.iter().filter(|r| !registry.matches(r)).collect();

    assert!(
        unresolved.len() >= 2,
        "Should flag both 'Mordecai' and 'Zephira' as unresolved; got {} unresolved: {:?} \
         (all refs: {:?})",
        unresolved.len(),
        unresolved,
        references
    );
}

// ===========================================================================
// AC-3 detail: OTEL warning includes the unresolved entity name
// ===========================================================================

#[test]
fn dispatch_otel_warning_includes_unresolved_name_field() {
    let source = include_str!("../../src/dispatch/mod.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);

    // The WatcherEventBuilder for entity_reference must include the unresolved
    // entity name as a field so the GM panel can display it.
    // Look for a .field() call containing "name" or "unresolved" or "entity"
    // near the "entity_reference" ValidationWarning emission.
    if let Some(pos) = production_code.find("\"entity_reference\"") {
        let window_end = (pos + 400_usize).min(production_code.len());
        let window = &production_code[pos..window_end];
        assert!(
            window.contains(".field("),
            "WatcherEventBuilder for entity_reference must include .field() calls \
             with the unresolved entity name — story 35-2 AC-3"
        );
    } else {
        panic!(
            "dispatch/mod.rs production code must contain \"entity_reference\" \
             WatcherEventBuilder — story 35-2 AC-3"
        );
    }
}

//! Story 37-3: Inventory extraction misses explicit pickup actions
//!
//! RED phase — these tests verify the two-pass inventory extraction system
//! correctly handles explicit pickup language ("picks up X and pockets it").
//!
//! Two failure paths to cover:
//! 1. Haiku extraction prompt — must give clear guidance for pickup phrases
//! 2. Narrator game_patch — must define items_gained schema and examples
//!
//! Additionally: OTEL events inventory.mutation_extracted and
//! inventory.mutation_missed must be emitted.

use sidequest_agents::inventory_extractor::{InventoryMutation, MutationAction};

// ============================================================================
// 1. InventoryMutation serde: pickup-style responses parse correctly
// ============================================================================

#[test]
fn parse_acquired_with_pickup_detail() {
    let json = r#"[{
        "item_name": "rusty key",
        "action": "acquired",
        "detail": "picked up from the ground and pocketed",
        "category": "quest",
        "gold": null
    }]"#;
    let mutations: Vec<InventoryMutation> = serde_json::from_str(json).unwrap();
    assert_eq!(mutations.len(), 1);
    assert_eq!(mutations[0].item_name, "rusty key");
    assert_eq!(mutations[0].action, MutationAction::Acquired);
    assert_eq!(
        mutations[0].detail,
        "picked up from the ground and pocketed"
    );
    assert_eq!(mutations[0].category.as_deref(), Some("quest"));
}

#[test]
fn parse_multiple_pickups_in_single_response() {
    let json = r#"[
        {"item_name": "rusty key", "action": "acquired", "detail": "picked up from floor", "category": "quest"},
        {"item_name": "torn letter", "action": "acquired", "detail": "found in pocket of the dead man", "category": "quest"}
    ]"#;
    let mutations: Vec<InventoryMutation> = serde_json::from_str(json).unwrap();
    assert_eq!(mutations.len(), 2);
    assert_eq!(mutations[0].action, MutationAction::Acquired);
    assert_eq!(mutations[1].action, MutationAction::Acquired);
    assert_eq!(mutations[1].item_name, "torn letter");
}

#[test]
fn parse_pickup_alongside_consumption() {
    // Player uses a potion AND picks up a new item in same narration
    let json = r#"[
        {"item_name": "Healing Potion", "action": "consumed", "detail": "drank it hastily"},
        {"item_name": "iron dagger", "action": "acquired", "detail": "picks up from fallen bandit", "category": "weapon"}
    ]"#;
    let mutations: Vec<InventoryMutation> = serde_json::from_str(json).unwrap();
    assert_eq!(mutations.len(), 2);
    assert_eq!(mutations[0].action, MutationAction::Consumed);
    assert_eq!(mutations[1].action, MutationAction::Acquired);
    assert_eq!(mutations[1].category.as_deref(), Some("weapon"));
}

// ============================================================================
// 2. Extraction prompt content — must explicitly guide Haiku on pickup language
// ============================================================================

/// The extraction prompt MUST include specific example narration phrases for
/// pickup actions, not just the word "picked up" in a bullet list.
/// Dev needs to make `build_extraction_prompt` pub and add example narration
/// that demonstrates compound pickup phrases Haiku must classify as acquired.
#[test]
fn extraction_prompt_includes_pickup_narration_examples() {
    // This tests that the prompt contains explicit narration examples like
    // "picks up X and pockets it" — not just the word in a list.
    // Dev must make build_extraction_prompt pub for this test to compile.
    let prompt = sidequest_agents::inventory_extractor::build_extraction_prompt(
        "I pick up the rusty key",
        "You crouch and pick up the rusty key, pocketing it safely.",
        &["Iron Sword".to_string()],
    );

    // Prompt must include explicit pickup narration examples
    assert!(
        prompt.contains("picks up") || prompt.contains("pockets"),
        "Extraction prompt must include compound pickup phrase examples"
    );

    // Prompt must include guidance about compound actions (verb + stow action)
    assert!(
        prompt.contains("pockets it")
            || prompt.contains("tucks it")
            || prompt.contains("stows it")
            || prompt.contains("adds it to"),
        "Extraction prompt must call out compound pickup+stow phrases as acquired"
    );
}

/// The prompt section listing acquired action types must include present-tense
/// pickup verbs, not just past-tense "picked up".
#[test]
fn extraction_prompt_acquired_section_covers_tense_variants() {
    let prompt = sidequest_agents::inventory_extractor::build_extraction_prompt(
        "I grab the flask",
        "You grab the flask from the shelf.",
        &[],
    );

    // Must cover present-tense pickup verbs the narrator commonly uses
    assert!(
        prompt.contains("picks up") || prompt.contains("grabs") || prompt.contains("takes"),
        "Acquired definition must include present-tense pickup verbs"
    );
}

// ============================================================================
// 3. Narrator game_patch — items_gained must have schema and example
// ============================================================================

/// The narrator output format MUST define the items_gained schema so the LLM
/// knows what shape to emit. Currently it only lists the field name.
#[test]
fn narrator_output_format_defines_items_gained_schema() {
    // Build the narrator's output format prompt section and verify it
    // includes a schema definition for items_gained (not just the field name
    // in a comma-separated list).
    let output_format = sidequest_agents::agents::narrator::narrator_output_format_text();

    // Must contain a definition of items_gained format
    // (not just "items_gained" as a word in a list of valid fields)
    assert!(
        output_format.contains("items_gained")
            && (output_format.contains("\"items_gained\": [")
                || output_format.contains("items_gained:")),
        "Narrator output format must define items_gained schema, not just list the field name"
    );
}

/// The narrator must have an example game_patch showing items_gained usage.
#[test]
fn narrator_output_format_includes_items_gained_example() {
    let output_format = sidequest_agents::agents::narrator::narrator_output_format_text();

    // Must have an example game_patch block that includes items_gained
    // Currently examples A/B/C don't show items_gained at all
    assert!(
        output_format.contains("items_gained")
            && output_format.contains("\"name\":")
            && output_format.contains("\"category\":"),
        "Narrator output format must include an items_gained example with name and category fields"
    );
}

/// The narrator prompt must instruct WHEN to emit items_gained —
/// specifically on turns where the player acquires items through action.
#[test]
fn narrator_output_format_instructs_when_to_emit_items_gained() {
    let output_format = sidequest_agents::agents::narrator::narrator_output_format_text();

    // Must explain when to use items_gained (like gold_change has explicit instructions)
    assert!(
        (output_format.contains("items_gained") && output_format.contains("acquire"))
            || (output_format.contains("items_gained") && output_format.contains("pick")),
        "Narrator output format must instruct when to emit items_gained (on acquisition)"
    );
}

// ============================================================================
// 4. GamePatchExtraction — items_gained deserialization from narrator JSON
// ============================================================================

/// Verify that a game_patch JSON block with items_gained deserializes correctly
/// through the narrator's extraction pipeline.
#[test]
fn game_patch_items_gained_deserializes() {
    // Test that the protocol ItemGained type handles narrator-style JSON
    let json = r#"{"name": "rusty key", "description": "A small iron key with rust spots", "category": "quest"}"#;
    let item: sidequest_protocol::ItemGained = serde_json::from_str(json).unwrap();
    assert_eq!(item.name.as_str(), "rusty key");
    assert_eq!(
        item.description.as_str(),
        "A small iron key with rust spots"
    );
    assert_eq!(item.category, "quest");
}

/// ItemGained should handle missing description with a default.
#[test]
fn game_patch_items_gained_default_description() {
    let json = r#"{"name": "rusty key", "category": "quest"}"#;
    let item: sidequest_protocol::ItemGained = serde_json::from_str(json).unwrap();
    assert_eq!(item.name.as_str(), "rusty key");
    // After the NonBlankString sweep the default_item_description helper
    // returns a non-blank `NonBlankString`, so the field is trivially
    // non-empty — the assertion remains as a regression guard.
    assert!(
        !item.description.as_str().is_empty(),
        "Default description must be non-empty"
    );
}

/// ItemGained should handle missing category with a default.
#[test]
fn game_patch_items_gained_default_category() {
    let json = r#"{"name": "mysterious orb"}"#;
    let item: sidequest_protocol::ItemGained = serde_json::from_str(json).unwrap();
    assert_eq!(item.name.as_str(), "mysterious orb");
    assert!(
        !item.category.is_empty(),
        "Default category must be non-empty"
    );
}

/// A full game_patch with items_gained array should parse.
#[test]
fn game_patch_with_items_gained_array() {
    let json = r#"{
        "items_gained": [
            {"name": "rusty key", "category": "quest"},
            {"name": "torn letter", "description": "A letter with a wax seal", "category": "quest"}
        ],
        "mood": "mysterious"
    }"#;

    // Parse as a generic Value to verify structure
    let patch: serde_json::Value = serde_json::from_str(json).unwrap();
    let items = patch["items_gained"].as_array().unwrap();
    assert_eq!(items.len(), 2);

    // Each item should deserialize to ItemGained — trivially non-empty
    // post-NonBlankString sweep, but kept as a regression guard.
    for item_val in items {
        let item: sidequest_protocol::ItemGained =
            serde_json::from_value(item_val.clone()).unwrap();
        assert!(!item.name.as_str().is_empty());
    }
}

// ============================================================================
// 5. OTEL events — inventory.mutation_extracted and inventory.mutation_missed
// ============================================================================

/// The inventory extractor must emit an OTEL event when a mutation is
/// successfully extracted. Dev must add `inventory.mutation_extracted` event
/// with fields: item_name, action, category, detail.
#[test]
fn otel_mutation_extracted_event_exists() {
    // This test verifies the OTEL event constant/function exists.
    // Dev must add it to inventory_extractor.rs or sidequest-telemetry.
    // Until then, this won't compile.
    let event_name = sidequest_agents::inventory_extractor::OTEL_MUTATION_EXTRACTED;
    assert_eq!(event_name, "inventory.mutation_extracted");
}

/// The inventory extractor must emit an OTEL event when extraction fails
/// to detect a mutation that the narration suggests should exist.
/// Dev must add `inventory.mutation_missed` event.
#[test]
fn otel_mutation_missed_event_exists() {
    let event_name = sidequest_agents::inventory_extractor::OTEL_MUTATION_MISSED;
    assert_eq!(event_name, "inventory.mutation_missed");
}

// ============================================================================
// 6. Integration: parse_extraction_response handles pickup narration JSON
//    (Dev must make parse_extraction_response pub)
// ============================================================================

/// parse_extraction_response must handle a realistic Haiku response for
/// explicit pickup narration ("picks up X and pockets it").
#[test]
fn parse_response_explicit_pickup_narration() {
    let haiku_response = r#"[{"item_name": "rusty key", "action": "acquired", "detail": "player picks up the rusty key from the ground and pockets it", "category": "quest", "gold": null}]"#;

    let mutations =
        sidequest_agents::inventory_extractor::parse_extraction_response(haiku_response)
            .expect("Explicit pickup narration must parse as acquisition");
    assert_eq!(mutations.len(), 1);
    assert_eq!(mutations[0].action, MutationAction::Acquired);
    assert_eq!(mutations[0].item_name, "rusty key");
}

/// parse_extraction_response must handle fenced pickup responses.
#[test]
fn parse_response_fenced_pickup() {
    let response = "Based on the narration, the player acquired an item:\n```json\n[{\"item_name\": \"old compass\", \"action\": \"acquired\", \"detail\": \"found and pocketed\", \"category\": \"tool\", \"gold\": null}]\n```";

    let mutations = sidequest_agents::inventory_extractor::parse_extraction_response(response)
        .expect("Fenced pickup response must parse");
    assert_eq!(mutations.len(), 1);
    assert_eq!(mutations[0].action, MutationAction::Acquired);
}

/// parse_extraction_response must handle the "finds a key" pattern.
#[test]
fn parse_response_finds_pattern() {
    let response = r#"[{"item_name": "silver ring", "action": "acquired", "detail": "finds it in the dust", "category": "treasure", "gold": null}]"#;

    let mutations = sidequest_agents::inventory_extractor::parse_extraction_response(response)
        .expect("'finds' pattern must parse as acquisition");
    assert_eq!(mutations.len(), 1);
    assert_eq!(mutations[0].action, MutationAction::Acquired);
}

/// parse_extraction_response must handle "loots X from the corpse" pattern.
#[test]
fn parse_response_loots_from_corpse() {
    let response = r#"[{"item_name": "iron sword", "action": "acquired", "detail": "loots from the fallen bandit's corpse", "category": "weapon", "gold": null}]"#;

    let mutations = sidequest_agents::inventory_extractor::parse_extraction_response(response)
        .expect("'loots from corpse' must parse as acquisition");
    assert_eq!(mutations.len(), 1);
    assert_eq!(mutations[0].action, MutationAction::Acquired);
    assert_eq!(mutations[0].category.as_deref(), Some("weapon"));
}

// ============================================================================
// 7. Wiring test — verify inventory extractor is imported and used in dispatch
// ============================================================================

/// Wiring sanity: the InventoryMutation and MutationAction types must be
/// re-exported from the crate root so dispatch can use them.
#[test]
fn inventory_extractor_types_are_public() {
    // If this compiles, the types are accessible from integration tests
    let mutation = InventoryMutation {
        item_name: "test".to_string(),
        action: MutationAction::Acquired,
        detail: "test".to_string(),
        category: Some("misc".to_string()),
        gold: None,
    };
    assert_eq!(mutation.action, MutationAction::Acquired);
}

/// MutationAction must round-trip through serde for all variants.
#[test]
fn mutation_action_serde_roundtrip() {
    let actions = vec![
        MutationAction::Consumed,
        MutationAction::Sold,
        MutationAction::Given,
        MutationAction::Lost,
        MutationAction::Destroyed,
        MutationAction::Acquired,
    ];

    for action in &actions {
        let json = serde_json::to_string(action).unwrap();
        let deserialized: MutationAction = serde_json::from_str(&json).unwrap();
        assert_eq!(&deserialized, action, "Roundtrip failed for {action}");
    }
}

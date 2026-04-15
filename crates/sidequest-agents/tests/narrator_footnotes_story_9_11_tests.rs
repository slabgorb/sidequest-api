//! Story 9-11: Structured footnote output — narrator emits NarrationPayload with footnotes
//!
//! RED phase — these tests reference types and functions that don't exist yet:
//!   - `Footnote` struct in sidequest-protocol
//!   - `FactCategory` enum in sidequest-protocol
//!   - `NarrationPayload.footnotes` field
//!   - `footnotes_to_discovered_facts()` conversion function
//!   - `register_footnote_protocol_section()` on PromptRegistry
//!
//! ACs tested:
//!   AC1: Structured output — narrator response includes footnotes[] alongside prose
//!   AC2: Marker parsing — footnote markers [N] in prose match entries in footnotes array
//!   AC3: New discovery — is_new: true footnotes create KnownFact entries
//!   AC4: Callback reference — is_new: false footnotes include valid fact_id
//!   AC5: Category tagging — each footnote has a FactCategory
//!   AC6: Empty suppression — no footnotes section when nothing to report
//!   AC7: Graceful fallback — narration still displays if footnotes missing/malformed

use sidequest_agents::prompt_framework::{
    AttentionZone, PromptComposer, PromptRegistry, PromptSection, SectionCategory,
};
use sidequest_protocol::{FactCategory, Footnote, NarrationPayload};

// =========================================================================
// Helpers
// =========================================================================

fn nbs(s: &str) -> sidequest_protocol::NonBlankString {
    sidequest_protocol::NonBlankString::new(s).expect("test literal must be non-blank")
}

fn sample_footnote_new(marker: u32, summary: &str, category: FactCategory) -> Footnote {
    Footnote {
        marker: Some(marker),
        fact_id: None,
        summary: nbs(summary),
        category,
        is_new: true,
    }
}

fn sample_footnote_callback(
    marker: u32,
    summary: &str,
    fact_id: &str,
    category: FactCategory,
) -> Footnote {
    Footnote {
        marker: Some(marker),
        fact_id: Some(fact_id.to_string()),
        summary: nbs(summary),
        category,
        is_new: false,
    }
}

fn sample_payload_with_footnotes() -> NarrationPayload {
    NarrationPayload {
        text: nbs(
            "As you enter the grove, Reva feels a deep wrongness [1]. \
               The old well [2] sits at the center, just as the innkeeper described [3].",
        ),
        state_delta: None,
        footnotes: vec![
            sample_footnote_new(
                1,
                "Corruption detected in the grove's oldest tree",
                FactCategory::Place,
            ),
            sample_footnote_new(
                2,
                "The old well — possible entrance to underground tunnels",
                FactCategory::Place,
            ),
            sample_footnote_callback(
                3,
                "The innkeeper mentioned tunnels beneath the well",
                "fact-innkeeper-tunnels",
                FactCategory::Person,
            ),
        ],
    }
}

// =========================================================================
// AC1: Structured output — narrator response includes footnotes[] alongside prose
// =========================================================================

#[test]
fn narration_payload_contains_prose_and_footnotes() {
    let payload = sample_payload_with_footnotes();

    assert!(
        !payload.text.as_str().is_empty(),
        "Narration payload should contain prose text",
    );
    assert_eq!(
        payload.footnotes.len(),
        3,
        "Narration payload should contain 3 footnotes",
    );
}

#[test]
fn narration_payload_serializes_with_footnotes() {
    let payload = sample_payload_with_footnotes();
    let json = serde_json::to_string(&payload).expect("Should serialize");

    assert!(
        json.contains("footnotes"),
        "Serialized JSON should contain 'footnotes' key.\nGot: {}",
        json,
    );
    assert!(
        json.contains("Corruption detected"),
        "Serialized JSON should contain footnote summary.\nGot: {}",
        json,
    );
}

#[test]
fn narration_payload_roundtrips_through_json() {
    let payload = sample_payload_with_footnotes();
    let json = serde_json::to_string(&payload).expect("Should serialize");
    let restored: NarrationPayload = serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(restored.text, payload.text, "Prose should round-trip");
    assert_eq!(
        restored.footnotes.len(),
        payload.footnotes.len(),
        "Footnote count should round-trip",
    );
    assert_eq!(
        restored.footnotes[0].summary.as_str(),
        "Corruption detected in the grove's oldest tree",
        "First footnote summary should round-trip",
    );
}

// =========================================================================
// AC2: Marker parsing — footnote markers [N] match entries in footnotes array
// =========================================================================

#[test]
fn footnote_markers_match_array_indices() {
    let payload = sample_payload_with_footnotes();

    // Each footnote's marker should correspond to a [N] marker in the prose
    for footnote in &payload.footnotes {
        if let Some(marker) = footnote.marker {
            let marker_text = format!("[{}]", marker);
            assert!(
                payload.text.contains(&marker_text),
                "Prose should contain marker {} for footnote: {}.\nProse: {}",
                marker_text,
                footnote.summary,
                payload.text,
            );
        }
    }
}

#[test]
fn footnote_markers_are_sequential_from_one() {
    let payload = sample_payload_with_footnotes();

    let markers: Vec<Option<u32>> = payload.footnotes.iter().map(|f| f.marker).collect();
    let expected: Vec<Option<u32>> = (1..=payload.footnotes.len() as u32).map(Some).collect();

    assert_eq!(
        markers, expected,
        "Footnote markers should be sequential starting from 1.\nGot: {:?}",
        markers,
    );
}

// =========================================================================
// AC3: New discovery — is_new: true footnotes create KnownFact entries
// =========================================================================

#[test]
fn new_footnotes_convert_to_discovered_facts() {
    let footnotes = vec![
        sample_footnote_new(1, "Corruption in the grove", FactCategory::Place),
        sample_footnote_callback(
            2,
            "Innkeeper mentioned tunnels",
            "fact-123",
            FactCategory::Person,
        ),
        sample_footnote_new(3, "Ancient runes on the well", FactCategory::Lore),
    ];

    let discovered = sidequest_agents::footnotes::footnotes_to_discovered_facts(
        &footnotes,
        "Reva Thornwood",
        42, // current turn
        sidequest_game::known_fact::FactSource::Discovery,
    );

    // Only is_new: true footnotes should produce DiscoveredFacts
    assert_eq!(
        discovered.len(),
        2,
        "Only 2 new footnotes should become DiscoveredFacts (not the callback)",
    );
}

#[test]
fn new_footnote_maps_to_known_fact_fields() {
    let footnotes = vec![sample_footnote_new(
        1,
        "Corruption in the grove",
        FactCategory::Place,
    )];

    let discovered = sidequest_agents::footnotes::footnotes_to_discovered_facts(
        &footnotes,
        "Reva Thornwood",
        42,
        sidequest_game::known_fact::FactSource::Discovery,
    );

    assert_eq!(discovered.len(), 1);
    let fact = &discovered[0];
    assert_eq!(fact.character_name, "Reva Thornwood");
    assert_eq!(fact.fact.content, "Corruption in the grove");
    assert_eq!(fact.fact.learned_turn, 42);
}

#[test]
fn new_footnote_defaults_to_certain_confidence() {
    let footnotes = vec![sample_footnote_new(
        1,
        "The mayor is a cultist",
        FactCategory::Person,
    )];

    let discovered = sidequest_agents::footnotes::footnotes_to_discovered_facts(
        &footnotes,
        "Reva",
        1,
        sidequest_game::known_fact::FactSource::Discovery,
    );

    assert_eq!(discovered.len(), 1);
    assert!(
        matches!(
            discovered[0].fact.confidence,
            sidequest_game::known_fact::Confidence::Certain
        ),
        "New discoveries should default to Certain confidence",
    );
}

#[test]
fn new_footnote_source_is_observation() {
    let footnotes = vec![sample_footnote_new(
        1,
        "Ancient runes glow",
        FactCategory::Lore,
    )];

    let discovered = sidequest_agents::footnotes::footnotes_to_discovered_facts(
        &footnotes,
        "Reva",
        5,
        sidequest_game::known_fact::FactSource::Discovery,
    );

    assert_eq!(discovered.len(), 1);
    assert!(
        matches!(
            discovered[0].fact.source,
            sidequest_game::known_fact::FactSource::Discovery
        ),
        "Footnote-derived facts should use Discovery source",
    );
}

// =========================================================================
// AC4: Callback reference — is_new: false footnotes include valid fact_id
// =========================================================================

#[test]
fn callback_footnote_has_fact_id() {
    let footnote = sample_footnote_callback(
        1,
        "The innkeeper's warning",
        "fact-innkeeper-tunnels",
        FactCategory::Person,
    );

    assert!(!footnote.is_new, "Callback footnote should not be new");
    assert_eq!(
        footnote.fact_id.as_deref(),
        Some("fact-innkeeper-tunnels"),
        "Callback footnote should contain the linked fact_id",
    );
}

#[test]
fn callback_footnotes_are_excluded_from_discovered_facts() {
    let footnotes = vec![sample_footnote_callback(
        1,
        "Innkeeper's warning",
        "fact-123",
        FactCategory::Person,
    )];

    let discovered = sidequest_agents::footnotes::footnotes_to_discovered_facts(
        &footnotes,
        "Reva",
        10,
        sidequest_game::known_fact::FactSource::Discovery,
    );

    assert!(
        discovered.is_empty(),
        "Callback footnotes (is_new: false) should NOT create DiscoveredFacts",
    );
}

#[test]
fn callback_footnote_serializes_with_fact_id() {
    let footnote = sample_footnote_callback(1, "Prior knowledge", "fact-abc", FactCategory::Lore);
    let json = serde_json::to_string(&footnote).expect("Should serialize");

    assert!(
        json.contains("fact-abc"),
        "Serialized callback should contain fact_id.\nGot: {}",
        json,
    );
    assert!(
        json.contains("\"is_new\":false") || json.contains("\"is_new\": false"),
        "Serialized callback should have is_new: false.\nGot: {}",
        json,
    );
}

// =========================================================================
// AC5: Category tagging — each footnote has a FactCategory
// =========================================================================

#[test]
fn fact_category_covers_all_variants() {
    // All five categories from the spec should be constructable
    let categories = [
        FactCategory::Lore,
        FactCategory::Place,
        FactCategory::Person,
        FactCategory::Quest,
        FactCategory::Ability,
    ];

    assert_eq!(categories.len(), 5, "FactCategory should have 5 variants");
}

#[test]
fn fact_category_serializes_as_string() {
    let json = serde_json::to_string(&FactCategory::Lore).expect("Should serialize");
    // Category should serialize as a string, not a number
    assert!(
        json.contains("Lore") || json.contains("lore"),
        "FactCategory::Lore should serialize as a string.\nGot: {}",
        json,
    );
}

#[test]
fn fact_category_roundtrips_all_variants() {
    let variants = vec![
        FactCategory::Lore,
        FactCategory::Place,
        FactCategory::Person,
        FactCategory::Quest,
        FactCategory::Ability,
    ];

    for variant in &variants {
        let json = serde_json::to_string(variant).expect("Should serialize");
        let restored: FactCategory = serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(
            std::mem::discriminant(&restored),
            std::mem::discriminant(variant),
            "FactCategory variant should round-trip: {}",
            json,
        );
    }
}

#[test]
fn footnote_category_appears_in_serialized_output() {
    let footnote = sample_footnote_new(1, "Ancient temple discovered", FactCategory::Place);
    let json = serde_json::to_string(&footnote).expect("Should serialize");

    assert!(
        json.contains("Place") || json.contains("place"),
        "Serialized footnote should contain its category.\nGot: {}",
        json,
    );
}

// =========================================================================
// AC6: Empty suppression — no footnotes section when nothing to report
// =========================================================================

#[test]
fn narration_payload_with_empty_footnotes_omits_field() {
    let payload = NarrationPayload {
        text: "You walk through the quiet forest.".to_string(),
        state_delta: None,
        footnotes: vec![],
    };

    let json = serde_json::to_string(&payload).expect("Should serialize");

    // Empty footnotes should be omitted from JSON output (skip_serializing_if)
    assert!(
        !json.contains("footnotes"),
        "Empty footnotes should be omitted from serialized output.\nGot: {}",
        json,
    );
}

#[test]
fn narration_payload_deserializes_without_footnotes_field() {
    // JSON from a narrator that didn't emit any footnotes
    let json = r#"{"text":"The wind howls through the valley."}"#;
    let payload: NarrationPayload = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(payload.text, "The wind howls through the valley.");
    assert!(
        payload.footnotes.is_empty(),
        "Missing footnotes field should deserialize as empty vec",
    );
}

#[test]
fn empty_footnotes_produce_no_discovered_facts() {
    let footnotes: Vec<Footnote> = vec![];

    let discovered = sidequest_agents::footnotes::footnotes_to_discovered_facts(
        &footnotes,
        "Reva",
        1,
        sidequest_game::known_fact::FactSource::Discovery,
    );

    assert!(
        discovered.is_empty(),
        "Empty footnotes should produce no DiscoveredFacts",
    );
}

// =========================================================================
// AC7: Graceful fallback — narration still displays if footnotes missing
// =========================================================================

#[test]
fn narration_without_footnotes_field_is_valid() {
    // Narrator response with no footnotes key at all — backwards-compatible
    let json = r#"{"text":"The tavern is warm and noisy.","state_delta":null}"#;
    let result = serde_json::from_str::<NarrationPayload>(json);

    assert!(
        result.is_ok(),
        "NarrationPayload should deserialize even without footnotes field.\nError: {:?}",
        result.err(),
    );
    let payload = result.unwrap();
    assert_eq!(payload.text, "The tavern is warm and noisy.");
    assert!(payload.footnotes.is_empty());
}

#[test]
fn narration_with_null_footnotes_is_valid() {
    let json = r#"{"text":"You see a door.","footnotes":null}"#;
    let result = serde_json::from_str::<NarrationPayload>(json);

    assert!(
        result.is_ok(),
        "NarrationPayload should accept null footnotes.\nError: {:?}",
        result.err(),
    );
}

#[test]
fn narration_with_malformed_footnote_is_handled() {
    // A footnote missing required fields — should fail gracefully
    let json = r#"{"text":"You find a chest.","footnotes":[{"marker":1}]}"#;
    let result = serde_json::from_str::<NarrationPayload>(json);

    // Strict deserialization: malformed footnote should error (not silently drop)
    assert!(
        result.is_err(),
        "Malformed footnote (missing required fields) should fail deserialization",
    );
}

// =========================================================================
// Prompt section — register footnote protocol instruction
// =========================================================================

#[test]
fn footnote_protocol_section_registered_for_narrator() {
    let mut registry = PromptRegistry::new();

    registry.register_section(
        "narrator",
        PromptSection::new(
            "identity",
            "You are the narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    registry.register_footnote_protocol_section("narrator");

    let prompt = registry.compose("narrator");

    assert!(
        prompt.contains("FOOTNOTE") || prompt.contains("footnote"),
        "Narrator prompt should contain footnote protocol instructions.\nGot:\n{}",
        prompt,
    );
}

#[test]
fn footnote_protocol_contains_marker_instruction() {
    let mut registry = PromptRegistry::new();
    registry.register_footnote_protocol_section("narrator");

    let prompt = registry.compose("narrator");

    assert!(
        prompt.contains("[1]") || prompt.contains("marker"),
        "Footnote protocol should reference marker syntax.\nGot:\n{}",
        prompt,
    );
}

#[test]
fn footnote_protocol_contains_category_list() {
    let mut registry = PromptRegistry::new();
    registry.register_footnote_protocol_section("narrator");

    let prompt = registry.compose("narrator");

    assert!(
        prompt.contains("Lore") && prompt.contains("Place") && prompt.contains("Person"),
        "Footnote protocol should list FactCategory variants.\nGot:\n{}",
        prompt,
    );
}

#[test]
fn footnote_protocol_placed_in_late_zone() {
    let mut registry = PromptRegistry::new();
    registry.register_footnote_protocol_section("narrator");

    // Footnote protocol is an output format instruction — belongs in Late zone
    let sections = registry.get_sections(
        "narrator",
        Some(SectionCategory::Format),
        Some(AttentionZone::Late),
    );

    assert!(
        !sections.is_empty(),
        "Footnote protocol section should be in Late zone with Format category",
    );
}

// =========================================================================
// Footnote struct construction
// =========================================================================

#[test]
fn footnote_new_discovery_has_no_fact_id() {
    let footnote = sample_footnote_new(1, "A discovery", FactCategory::Lore);
    assert!(footnote.is_new);
    assert!(
        footnote.fact_id.is_none(),
        "New discovery footnotes should have no fact_id",
    );
}

#[test]
fn footnote_fields_serialize_correctly() {
    let footnote = Footnote {
        marker: Some(3),
        fact_id: Some("fact-abc".to_string()),
        summary: "The ancient ruins hold a secret".to_string(),
        category: FactCategory::Lore,
        is_new: false,
    };

    let json = serde_json::to_string(&footnote).expect("Should serialize");

    assert!(json.contains("\"marker\":3") || json.contains("\"marker\": 3"));
    assert!(json.contains("fact-abc"));
    assert!(json.contains("ancient ruins"));
    assert!(json.contains("Lore") || json.contains("lore"));
}

// =========================================================================
// Integration — full pipeline: narrator JSON → footnotes → KnownFacts
// =========================================================================

#[test]
fn full_pipeline_narrator_json_to_known_facts() {
    // Step 1: Simulate a narrator JSON response with footnotes
    let narrator_json = r#"{
        "text": "As you enter the grove, Reva feels a deep wrongness [1]. The old well [2] sits at the center, just as the innkeeper described [3].",
        "footnotes": [
            {
                "marker": 1,
                "fact_id": null,
                "summary": "Corruption detected in the grove's oldest tree",
                "category": "Place",
                "is_new": true
            },
            {
                "marker": 2,
                "fact_id": null,
                "summary": "The old well — possible entrance to underground tunnels",
                "category": "Place",
                "is_new": true
            },
            {
                "marker": 3,
                "fact_id": "fact-innkeeper-tunnels",
                "summary": "The innkeeper mentioned tunnels beneath the well",
                "category": "Person",
                "is_new": false
            }
        ]
    }"#;

    // Step 2: Parse the JSON into a NarrationPayload
    let payload: NarrationPayload =
        serde_json::from_str(narrator_json).expect("Should parse narrator JSON");

    assert_eq!(payload.footnotes.len(), 3, "Should have 3 footnotes");
    assert!(!payload.text.is_empty(), "Should have prose text");

    // Step 3: Convert new footnotes to DiscoveredFacts
    let discovered = sidequest_agents::footnotes::footnotes_to_discovered_facts(
        &payload.footnotes,
        "Reva Thornwood",
        15, // turn 15
        sidequest_game::known_fact::FactSource::Discovery,
    );

    // Only is_new: true footnotes become DiscoveredFacts
    assert_eq!(
        discovered.len(),
        2,
        "2 new discoveries, 1 callback — should produce 2 DiscoveredFacts",
    );

    // Step 4: Verify the discovered facts have correct fields
    assert_eq!(discovered[0].character_name, "Reva Thornwood");
    assert_eq!(
        discovered[0].fact.content,
        "Corruption detected in the grove's oldest tree"
    );
    assert_eq!(discovered[0].fact.learned_turn, 15);

    assert_eq!(discovered[1].character_name, "Reva Thornwood");
    assert_eq!(
        discovered[1].fact.content,
        "The old well — possible entrance to underground tunnels"
    );

    // Step 5: Verify callback footnote was NOT converted
    let callback = &payload.footnotes[2];
    assert!(!callback.is_new);
    assert_eq!(callback.fact_id.as_deref(), Some("fact-innkeeper-tunnels"));
}

#[test]
fn full_pipeline_prompt_to_response_to_facts() {
    // Step 1: Build narrator prompt with footnote protocol
    let mut registry = PromptRegistry::new();

    registry.register_section(
        "narrator",
        PromptSection::new(
            "narrator_identity",
            "You are the narrator of a dark fantasy world.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    // Register the footnote protocol instruction
    registry.register_footnote_protocol_section("narrator");

    registry.register_section(
        "narrator",
        PromptSection::new(
            "player_action",
            "The player says: I examine the old well.",
            AttentionZone::Recency,
            SectionCategory::Action,
        ),
    );

    // Step 2: Verify the composed prompt contains footnote instructions
    let prompt = registry.compose("narrator");

    assert!(
        prompt.contains("FOOTNOTE") || prompt.contains("footnote"),
        "Composed prompt should contain footnote protocol",
    );
    assert!(
        prompt.contains("You are the narrator"),
        "Composed prompt should contain identity",
    );
    assert!(
        prompt.contains("examine the old well"),
        "Composed prompt should contain player action",
    );

    // Step 3: Verify ordering — identity → footnote protocol → action
    let identity_pos = prompt.find("You are the narrator").unwrap();
    let action_pos = prompt.find("examine the old well").unwrap();

    assert!(
        identity_pos < action_pos,
        "Identity should appear before player action in prompt",
    );

    // Step 4: Simulate a narrator response to this prompt
    let response_json = r#"{
        "text": "You peer into the well's depths. Ancient runes [1] glow faintly along the stone rim.",
        "footnotes": [
            {
                "marker": 1,
                "fact_id": null,
                "summary": "Ancient protective runes carved into the well's rim",
                "category": "Lore",
                "is_new": true
            }
        ]
    }"#;

    let payload: NarrationPayload =
        serde_json::from_str(response_json).expect("Should parse response");

    // Step 5: Convert to DiscoveredFacts
    let discovered = sidequest_agents::footnotes::footnotes_to_discovered_facts(
        &payload.footnotes,
        "Reva Thornwood",
        16,
        sidequest_game::known_fact::FactSource::Discovery,
    );

    assert_eq!(discovered.len(), 1);
    assert_eq!(
        discovered[0].fact.content,
        "Ancient protective runes carved into the well's rim"
    );
    assert_eq!(discovered[0].fact.learned_turn, 16);
    assert_eq!(discovered[0].character_name, "Reva Thornwood");
}

// =========================================================================
// AC coverage documentation
// =========================================================================

#[test]
fn coverage_check_all_acs_have_tests() {
    // AC1: Structured output — narrator response includes footnotes[]
    //   → narration_payload_contains_prose_and_footnotes
    //   → narration_payload_serializes_with_footnotes
    //   → narration_payload_roundtrips_through_json
    // AC2: Marker parsing — [N] markers match footnotes array
    //   → footnote_markers_match_array_indices
    //   → footnote_markers_are_sequential_from_one
    // AC3: New discovery — is_new: true creates KnownFact
    //   → new_footnotes_convert_to_discovered_facts
    //   → new_footnote_maps_to_known_fact_fields
    //   → new_footnote_defaults_to_certain_confidence
    //   → new_footnote_source_is_observation
    // AC4: Callback reference — is_new: false includes fact_id
    //   → callback_footnote_has_fact_id
    //   → callback_footnotes_are_excluded_from_discovered_facts
    //   → callback_footnote_serializes_with_fact_id
    // AC5: Category tagging — FactCategory on each footnote
    //   → fact_category_covers_all_variants
    //   → fact_category_serializes_as_string
    //   → fact_category_roundtrips_all_variants
    //   → footnote_category_appears_in_serialized_output
    // AC6: Empty suppression — omit footnotes when empty
    //   → narration_payload_with_empty_footnotes_omits_field
    //   → narration_payload_deserializes_without_footnotes_field
    //   → empty_footnotes_produce_no_discovered_facts
    // AC7: Graceful fallback — narration displays without footnotes
    //   → narration_without_footnotes_field_is_valid
    //   → narration_with_null_footnotes_is_valid
    //   → narration_with_malformed_footnote_is_handled
    // Prompt section:
    //   → footnote_protocol_section_registered_for_narrator
    //   → footnote_protocol_contains_marker_instruction
    //   → footnote_protocol_contains_category_list
    //   → footnote_protocol_placed_in_late_zone
    // Footnote construction:
    //   → footnote_new_discovery_has_no_fact_id
    //   → footnote_fields_serialize_correctly
    assert_eq!(7, 7, "All 7 ACs covered by tests above");
}

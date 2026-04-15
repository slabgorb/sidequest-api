//! Story 33-18: Scrapbook payload — ScrapbookEntry WebSocket message type
//!
//! RED-phase tests for a new `GameMessage::ScrapbookEntry` variant that
//! bundles per-turn metadata (turn_id, location, narration excerpt,
//! world facts, NPCs present, optional image) into one atomic message.
//!
//! This replaces the current UI flow where the Gallery/Scrapbook widget
//! has to stitch together `IMAGE`, `NARRATION` footnotes, and observer
//! state from separate streams. Story 33-17 consumes this payload.
//!
//! # AC coverage
//! - AC1: `ScrapbookEntry` variant on `GameMessage` with screaming-case tag
//! - AC2: `ScrapbookEntryPayload` holds all bundled fields
//! - AC3: Serde round-trip preserves every field
//! - AC4: `deny_unknown_fields` rejects schema drift
//! - AC5: `NpcRef` struct with name / role / disposition
//! - AC6: `world_facts` and `npcs_present` default to empty when absent
//! - AC7: `image_url`, `scene_title`, `scene_type` are `Option` (see
//!   Design Deviation — the AC text says `String` but images arrive on a
//!   separate async channel and may not exist at narration-end time)
//!
//! # Rule coverage
//! - No vacuous assertions — each test compares against concrete expected values
//! - `deny_unknown_fields` is enforced on both outer payload and `NpcRef`
//! - `#[non_exhaustive]` on `GameMessage` is preserved (adding a variant must
//!   not break existing pattern-match coverage)

use super::*;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn sample_npc_ref() -> NpcRef {
    NpcRef {
        name: "Toggler Copperjaw".to_string(),
        role: "blacksmith".to_string(),
        disposition: "gruff but fair".to_string(),
    }
}

fn sample_payload() -> ScrapbookEntryPayload {
    ScrapbookEntryPayload {
        turn_id: 7,
        scene_title: Some("The Forge of Broken Oaths".to_string()),
        scene_type: Some("exploration".to_string()),
        location: "Ironhold Market".to_string(),
        image_url: Some("/renders/turn-7.png".to_string()),
        narrative_excerpt: "The hammer rang once against cold iron.".to_string(),
        world_facts: vec![
            "The forge has been cold for six days.".to_string(),
            "Ironhold's smith guild disbanded last winter.".to_string(),
        ],
        npcs_present: vec![sample_npc_ref()],
    }
}

// ===========================================================================
// AC1 + AC2: variant exists, payload has all fields
// ===========================================================================

#[test]
fn scrapbook_entry_variant_constructs_and_matches() {
    let msg = GameMessage::ScrapbookEntry {
        payload: sample_payload(),
        player_id: "server".to_string(),
    };

    match &msg {
        GameMessage::ScrapbookEntry { payload, player_id } => {
            assert_eq!(player_id, "server");
            assert_eq!(payload.turn_id, 7);
            assert_eq!(
                payload.scene_title.as_deref(),
                Some("The Forge of Broken Oaths")
            );
            assert_eq!(payload.scene_type.as_deref(), Some("exploration"));
            assert_eq!(payload.location, "Ironhold Market");
            assert_eq!(payload.image_url.as_deref(), Some("/renders/turn-7.png"));
            assert_eq!(
                payload.narrative_excerpt,
                "The hammer rang once against cold iron."
            );
            assert_eq!(payload.world_facts.len(), 2);
            assert_eq!(payload.npcs_present.len(), 1);
            assert_eq!(payload.npcs_present[0].name, "Toggler Copperjaw");
        }
        _ => panic!("Expected ScrapbookEntry variant"),
    }
}

// ===========================================================================
// AC1: wire-format tag is SCREAMING_CASE per protocol convention
// ===========================================================================

#[test]
fn scrapbook_entry_serializes_with_screaming_case_tag() {
    let msg = GameMessage::ScrapbookEntry {
        payload: sample_payload(),
        player_id: "server".to_string(),
    };

    let json = serde_json::to_value(&msg).expect("serialize");
    assert_eq!(
        json.get("type").and_then(|v| v.as_str()),
        Some("SCRAPBOOK_ENTRY"),
        "GameMessage::ScrapbookEntry must serialize with type=\"SCRAPBOOK_ENTRY\" \
         (matches the other SCREAMING_CASE wire tags in the protocol)"
    );
}

// ===========================================================================
// AC3: serde round-trip preserves every field
// ===========================================================================

#[test]
fn scrapbook_entry_full_round_trip() {
    let original = GameMessage::ScrapbookEntry {
        payload: sample_payload(),
        player_id: "server".to_string(),
    };
    let encoded = serde_json::to_string(&original).expect("serialize");
    let decoded: GameMessage = serde_json::from_str(&encoded).expect("deserialize");
    assert_eq!(original, decoded);
}

#[test]
fn scrapbook_entry_payload_round_trip_preserves_world_facts_order() {
    let payload = ScrapbookEntryPayload {
        turn_id: 3,
        scene_title: None,
        scene_type: None,
        location: "Whispering Reach".to_string(),
        image_url: None,
        narrative_excerpt: "A single candle burned.".to_string(),
        world_facts: vec!["Fact A".to_string(), "Fact B".to_string(), "Fact C".to_string()],
        npcs_present: vec![],
    };
    let encoded = serde_json::to_string(&payload).expect("serialize");
    let decoded: ScrapbookEntryPayload = serde_json::from_str(&encoded).expect("deserialize");
    assert_eq!(decoded.world_facts, vec!["Fact A", "Fact B", "Fact C"]);
}

// ===========================================================================
// AC4: deny_unknown_fields — schema stability
// ===========================================================================

#[test]
fn scrapbook_entry_payload_rejects_unknown_fields() {
    let json = r#"{
        "turn_id": 1,
        "location": "Nowhere",
        "narrative_excerpt": "Quiet.",
        "world_facts": [],
        "npcs_present": [],
        "bogus_field": "should fail"
    }"#;
    let result: Result<ScrapbookEntryPayload, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "ScrapbookEntryPayload must use #[serde(deny_unknown_fields)] so schema drift \
         between server and client fails loudly instead of silently dropping fields"
    );
}

#[test]
fn npc_ref_rejects_unknown_fields() {
    let json = r#"{
        "name": "Vera",
        "role": "guard",
        "disposition": "wary",
        "extra": "nope"
    }"#;
    let result: Result<NpcRef, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "NpcRef must use #[serde(deny_unknown_fields)] to prevent silent schema drift"
    );
}

// ===========================================================================
// AC5: NpcRef shape
// ===========================================================================

#[test]
fn npc_ref_has_name_role_disposition_fields() {
    let npc = sample_npc_ref();
    assert_eq!(npc.name, "Toggler Copperjaw");
    assert_eq!(npc.role, "blacksmith");
    assert_eq!(npc.disposition, "gruff but fair");
}

#[test]
fn npc_ref_round_trip() {
    let original = sample_npc_ref();
    let encoded = serde_json::to_string(&original).expect("serialize");
    let decoded: NpcRef = serde_json::from_str(&encoded).expect("deserialize");
    assert_eq!(original, decoded);
}

// ===========================================================================
// AC6: world_facts / npcs_present default to empty when missing from JSON
// ===========================================================================

#[test]
fn scrapbook_entry_payload_world_facts_defaults_to_empty() {
    let json = r#"{
        "turn_id": 2,
        "location": "Hollow Rise",
        "narrative_excerpt": "Nothing stirred."
    }"#;
    let decoded: ScrapbookEntryPayload =
        serde_json::from_str(json).expect("minimal payload must deserialize");
    assert!(
        decoded.world_facts.is_empty(),
        "world_facts must default to an empty Vec when absent from JSON"
    );
    assert!(
        decoded.npcs_present.is_empty(),
        "npcs_present must default to an empty Vec when absent from JSON"
    );
}

// ===========================================================================
// AC7: image_url / scene_title / scene_type are optional
// (deviation from session AC — see session Design Deviations)
// ===========================================================================

#[test]
fn scrapbook_entry_payload_image_url_is_optional() {
    let json = r#"{
        "turn_id": 4,
        "location": "Ghost Harbor",
        "narrative_excerpt": "Fog rolled in."
    }"#;
    let decoded: ScrapbookEntryPayload = serde_json::from_str(json).expect("deserialize");
    assert!(
        decoded.image_url.is_none(),
        "image_url must be Option<String> — ScrapbookEntry is emitted at narration-end \
         time but images arrive on an async render channel and may not yet exist"
    );
    assert!(decoded.scene_title.is_none());
    assert!(decoded.scene_type.is_none());
}

#[test]
fn scrapbook_entry_payload_skips_none_on_serialize() {
    // Ensures we aren't sending `"image_url": null` over the wire when absent.
    // Existing payloads in this crate use skip_serializing_if — stay consistent.
    let payload = ScrapbookEntryPayload {
        turn_id: 11,
        scene_title: None,
        scene_type: None,
        location: "Empty".to_string(),
        image_url: None,
        narrative_excerpt: "Silence.".to_string(),
        world_facts: vec![],
        npcs_present: vec![],
    };
    let json = serde_json::to_value(&payload).expect("serialize");
    let obj = json.as_object().expect("object");
    assert!(
        !obj.contains_key("image_url"),
        "image_url: None must be skipped on serialize (skip_serializing_if), \
         matching the pattern used by ImagePayload / NarrationPayload"
    );
    assert!(!obj.contains_key("scene_title"));
    assert!(!obj.contains_key("scene_type"));
}

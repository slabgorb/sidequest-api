/// Story 29-11: Narrator tactical_place tool
///
/// RED phase — failing tests for the tactical_place tool that allows
/// the narrator to place entities on the tactical grid via tool calls.
/// Tests cover: tool validation, bounds checking, overlap detection,
/// grid summary generation, OTEL spans, and sidecar parsing.
use serde_json::json;

// ══════════════════════════════════════════════════════════════════════════════
// AC-1: Tool definition — tactical_place exists and is callable
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn validate_tactical_place_accepts_valid_params() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    let result = validate_tactical_place(
        "goblin-01",
        3,
        4,         // x, y
        "medium",  // size
        "hostile", // faction
        8,
        8,   // grid width, height
        &[], // existing entities (none)
    );

    assert!(
        result.is_ok(),
        "valid params should succeed: {:?}",
        result.err()
    );
    let place = result.unwrap();
    assert_eq!(place.entity_id, "goblin-01");
    assert_eq!(place.x, 3);
    assert_eq!(place.y, 4);
    assert_eq!(place.size, 1); // medium = 1 cell
    assert_eq!(place.faction, "hostile");
}

#[test]
fn validate_tactical_place_returns_entity_name_fields() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    let result = validate_tactical_place("pc-tormund", 2, 2, "medium", "player", 5, 5, &[]);

    let place = result.unwrap();
    assert_eq!(place.entity_id, "pc-tormund");
    assert_eq!(place.faction, "player");
}

// ══════════════════════════════════════════════════════════════════════════════
// AC-2: Placement validation — bounds, size, faction, overlap
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn rejects_x_out_of_bounds() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    let result = validate_tactical_place("npc-01", 10, 3, "medium", "hostile", 8, 8, &[]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("bounds"), "error should mention bounds: {err}");
}

#[test]
fn rejects_y_out_of_bounds() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    let result = validate_tactical_place("npc-01", 3, 10, "medium", "hostile", 8, 8, &[]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("bounds"), "error should mention bounds: {err}");
}

#[test]
fn rejects_invalid_size_string() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    let result = validate_tactical_place("npc-01", 3, 3, "gargantuan", "hostile", 8, 8, &[]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("size"), "error should mention size: {err}");
}

#[test]
fn rejects_invalid_faction_string() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    let result = validate_tactical_place("npc-01", 3, 3, "medium", "chaotic", 8, 8, &[]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("faction"),
        "error should mention faction: {err}"
    );
}

#[test]
fn accepts_all_valid_sizes() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    for (size_str, expected_span) in [("medium", 1u32), ("large", 2), ("huge", 3)] {
        let result = validate_tactical_place("entity", 0, 0, size_str, "neutral", 10, 10, &[]);
        assert!(result.is_ok(), "size '{size_str}' should be valid");
        assert_eq!(result.unwrap().size, expected_span);
    }
}

#[test]
fn accepts_all_valid_factions() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    for faction in ["player", "hostile", "neutral", "ally"] {
        let result = validate_tactical_place("entity", 0, 0, "medium", faction, 10, 10, &[]);
        assert!(result.is_ok(), "faction '{faction}' should be valid");
        assert_eq!(result.unwrap().faction, faction);
    }
}

#[test]
fn rejects_large_entity_extending_past_grid_edge() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    // Large (2x2) at position (7,7) on 8x8 grid extends to (8,8) — out of bounds
    let result = validate_tactical_place("ogre", 7, 7, "large", "hostile", 8, 8, &[]);
    assert!(
        result.is_err(),
        "large entity at edge should fail bounds check"
    );
}

#[test]
fn rejects_overlapping_entities() {
    use sidequest_agents::tools::tactical_place::{validate_tactical_place, PlacedEntity};

    let existing = vec![PlacedEntity {
        entity_id: "goblin-01".into(),
        x: 3,
        y: 4,
        size: 1,
    }];

    // Try to place another entity at the same position
    let result = validate_tactical_place("goblin-02", 3, 4, "medium", "hostile", 8, 8, &existing);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("overlap") || err.contains("occupied"),
        "error should mention overlap: {err}"
    );
}

#[test]
fn rejects_large_entity_overlapping_medium() {
    use sidequest_agents::tools::tactical_place::{validate_tactical_place, PlacedEntity};

    let existing = vec![PlacedEntity {
        entity_id: "guard".into(),
        x: 4,
        y: 4,
        size: 1,
    }];

    // Large (2x2) at (3,3) occupies cells (3,3), (4,3), (3,4), (4,4) — overlaps guard at (4,4)
    let result = validate_tactical_place("ogre", 3, 3, "large", "hostile", 8, 8, &existing);
    assert!(
        result.is_err(),
        "large entity should overlap with existing medium at (4,4)"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// AC-3: Entity registry — placed entities stored in TacticalStatePayload
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn tactical_place_result_converts_to_entity_payload() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;
    use sidequest_protocol::TacticalEntityPayload;

    let result =
        validate_tactical_place("npc-wizard", 5, 2, "medium", "ally", 10, 10, &[]).unwrap();

    let payload: TacticalEntityPayload = result.to_entity_payload("Wizard Gandara");
    assert_eq!(payload.id, "npc-wizard");
    assert_eq!(payload.name, "Wizard Gandara");
    assert_eq!(payload.x, 5);
    assert_eq!(payload.y, 2);
    assert_eq!(payload.size, 1);
    assert_eq!(payload.faction, "ally");
}

// ══════════════════════════════════════════════════════════════════════════════
// AC-4: Grid summary in narrator prompt
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn grid_summary_includes_entity_positions() {
    use sidequest_agents::tools::tactical_place::format_grid_summary;
    use sidequest_protocol::TacticalEntityPayload;

    let entities = vec![
        TacticalEntityPayload {
            id: "pc-01".into(),
            name: "Tormund".into(),
            x: 4,
            y: 3,
            size: 1,
            faction: "player".into(),
        },
        TacticalEntityPayload {
            id: "goblin-01".into(),
            name: "Grik".into(),
            x: 2,
            y: 5,
            size: 2,
            faction: "hostile".into(),
        },
    ];

    let summary = format_grid_summary(8, 8, &entities);

    assert!(
        summary.contains("Tormund"),
        "summary should include entity names"
    );
    assert!(
        summary.contains("4,3") || summary.contains("[4, 3]") || summary.contains("(4,3)"),
        "summary should include positions: {summary}"
    );
    assert!(
        summary.contains("player"),
        "summary should include factions: {summary}"
    );
    assert!(
        summary.contains("hostile"),
        "summary should include hostile faction: {summary}"
    );
}

#[test]
fn grid_summary_empty_when_no_entities() {
    use sidequest_agents::tools::tactical_place::format_grid_summary;

    let summary = format_grid_summary(8, 8, &[]);
    // Must indicate the grid dimensions and emptiness — not silently return ""
    assert!(
        summary.contains("8") && summary.contains("empty"),
        "empty grid summary must show dimensions AND emptiness indicator: {summary}"
    );
}

#[test]
fn grid_summary_shows_size_labels() {
    use sidequest_agents::tools::tactical_place::format_grid_summary;
    use sidequest_protocol::TacticalEntityPayload;

    let entities = vec![TacticalEntityPayload {
        id: "dragon".into(),
        name: "Red Dragon".into(),
        x: 1,
        y: 1,
        size: 3,
        faction: "hostile".into(),
    }];

    let summary = format_grid_summary(10, 10, &entities);
    // Should indicate size — "Huge" or "3x3" or similar
    assert!(
        summary.to_lowercase().contains("huge") || summary.contains("3"),
        "summary should indicate entity size: {summary}"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// AC-5: Wiring — sidecar JSONL parsing for tactical_place
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn sidecar_parser_populates_tactical_placements_from_valid_record() {
    use sidequest_agents::tools::tool_call_parser::{parse_tool_results, sidecar_path};

    // Write a valid tactical_place record to a temp sidecar file
    let session_id = "test-tactical-place-valid-29-11";
    let path = sidecar_path(session_id);
    // Ensure the sidecar directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create sidecar dir");
    }
    let record = json!({
        "tool": "tactical_place",
        "result": {
            "entity_id": "goblin-01",
            "x": 3,
            "y": 4,
            "size": 1,
            "faction": "hostile",
            "valid": true
        }
    });
    std::fs::write(&path, format!("{}\n", record)).expect("failed to write sidecar file");

    let results = parse_tool_results(session_id);

    // Cleanup
    let _ = std::fs::remove_file(&path);

    // The sidecar parser must populate tactical_placements with the parsed record
    assert!(
        results.tactical_placements.is_some(),
        "tactical_placements should be Some after parsing a valid tactical_place record"
    );
    let placements = results.tactical_placements.unwrap();
    assert_eq!(placements.len(), 1, "should have exactly 1 placement");
    assert_eq!(placements[0].entity_id, "goblin-01");
    assert_eq!(placements[0].x, 3);
    assert_eq!(placements[0].y, 4);
    assert_eq!(placements[0].size, 1);
    assert_eq!(placements[0].faction, "hostile");
}

#[test]
fn sidecar_parser_skips_invalid_tactical_place_record() {
    use sidequest_agents::tools::tool_call_parser::{parse_tool_results, sidecar_path};

    // Write a tactical_place record with valid: false
    let session_id = "test-tactical-place-invalid-29-11";
    let path = sidecar_path(session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create sidecar dir");
    }
    let record = json!({
        "tool": "tactical_place",
        "result": {
            "entity_id": "goblin-01",
            "x": 99,
            "y": 99,
            "size": 1,
            "faction": "hostile",
            "valid": false
        }
    });
    std::fs::write(&path, format!("{}\n", record)).expect("failed to write sidecar file");

    let results = parse_tool_results(session_id);

    // Cleanup
    let _ = std::fs::remove_file(&path);

    // Invalid placements should NOT be added to tactical_placements
    assert!(
        results.tactical_placements.is_none(),
        "tactical_placements should be None when all records are invalid"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// AC-6: OTEL span — tool.tactical_place
// ══════════════════════════════════════════════════════════════════════════════

// Removed: validate_function_has_tracing_instrument was vacuous (duplicated AC-1,
// didn't test OTEL). The #[instrument] attribute is verified by source inspection:
// tactical_place.rs:58-62 has #[tracing::instrument(name = "tool.tactical_place", ...)]
// with all 7 fields. Compile-time presence is sufficient.

#[test]
fn invalid_placement_carries_error_reason() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    let err =
        validate_tactical_place("otel-test", 99, 99, "medium", "player", 5, 5, &[]).unwrap_err();

    // Error reason should be non-empty (appears in OTEL span as error_reason)
    assert!(!err.is_empty(), "error reason must be non-empty for OTEL");
}

// ══════════════════════════════════════════════════════════════════════════════
// AC-3/AC-5: Wiring — tactical_placements flows through assemble_turn to ActionResult
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn assemble_turn_includes_tactical_placements_in_action_result() {
    use sidequest_agents::orchestrator::{ActionFlags, ActionRewrite, NarratorExtraction};
    use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};
    use sidequest_agents::tools::tactical_place::TacticalPlaceResult;
    use std::collections::HashMap;

    // Build a ToolCallResults with one tactical placement
    let tool_results = ToolCallResults {
        tactical_placements: Some(vec![TacticalPlaceResult {
            entity_id: "goblin-01".to_string(),
            x: 3,
            y: 4,
            size: 1,
            faction: "hostile".to_string(),
        }]),
        ..Default::default()
    };

    let extraction = NarratorExtraction {
        prose: "A goblin charges from the shadows.".to_string(),
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: None,
        scene_mood: None,
        personality_events: vec![],
        scene_intent: None,
        resource_deltas: HashMap::new(),
        lore_established: None,
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
        beat_selections: vec![],
        confrontation: None,
        location: None,
        affinity_progress: vec![],
        gold_change: None,
    };
    let rewrite = ActionRewrite::default();
    let flags = ActionFlags::default();

    let result = assemble_turn(extraction, rewrite, flags, tool_results);

    // ActionResult must carry tactical_placements through — this is the wiring test.
    // If ActionResult doesn't have a tactical_placements field, this won't compile.
    assert!(
        result.tactical_placements.is_some(),
        "ActionResult must carry tactical_placements from ToolCallResults"
    );
    let placements = result.tactical_placements.unwrap();
    assert_eq!(placements.len(), 1);
    assert_eq!(placements[0].entity_id, "goblin-01");
}

// ══════════════════════════════════════════════════════════════════════════════
// Lang-review rules
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn size_validation_is_case_insensitive() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    // Tool calls from Claude may have varying case
    for size in ["Medium", "MEDIUM", "medium", "Large", "HUGE"] {
        let result = validate_tactical_place("entity", 0, 0, size, "neutral", 10, 10, &[]);
        assert!(
            result.is_ok(),
            "size '{size}' should be accepted (case-insensitive)"
        );
    }
}

#[test]
fn faction_validation_is_case_insensitive() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    for faction in ["Player", "HOSTILE", "neutral", "Ally"] {
        let result = validate_tactical_place("entity", 0, 0, "medium", faction, 10, 10, &[]);
        assert!(
            result.is_ok(),
            "faction '{faction}' should be accepted (case-insensitive)"
        );
    }
}

#[test]
fn empty_entity_id_rejected() {
    use sidequest_agents::tools::tactical_place::validate_tactical_place;

    let result = validate_tactical_place("", 3, 3, "medium", "hostile", 8, 8, &[]);
    assert!(result.is_err(), "empty entity_id should be rejected");
}

// ══════════════════════════════════════════════════════════════════════════════
// Reviewer rework: duplicate entity_id guard
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn rejects_duplicate_entity_id() {
    use sidequest_agents::tools::tactical_place::{validate_tactical_place, PlacedEntity};

    let existing = vec![PlacedEntity {
        entity_id: "goblin-01".into(),
        x: 3,
        y: 4,
        size: 1,
    }];

    // Place the SAME entity_id at a different, non-overlapping position.
    // This should be rejected — same entity cannot exist in two places.
    let result = validate_tactical_place("goblin-01", 0, 0, "medium", "hostile", 8, 8, &existing);
    assert!(
        result.is_err(),
        "duplicate entity_id should be rejected even at non-overlapping position"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("already") || err.contains("duplicate") || err.contains("exists"),
        "error should mention entity already placed: {err}"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Reviewer rework: sidecar parser with multiple records
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn sidecar_parser_handles_multiple_tactical_place_records() {
    use sidequest_agents::tools::tool_call_parser::{parse_tool_results, sidecar_path};

    let session_id = "test-tactical-place-multi-29-11";
    let path = sidecar_path(session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create sidecar dir");
    }
    let record1 = json!({
        "tool": "tactical_place",
        "result": {
            "entity_id": "goblin-01", "x": 3, "y": 4, "size": 1,
            "faction": "hostile", "valid": true
        }
    });
    let record2 = json!({
        "tool": "tactical_place",
        "result": {
            "entity_id": "goblin-02", "x": 5, "y": 6, "size": 2,
            "faction": "hostile", "valid": true
        }
    });
    std::fs::write(&path, format!("{}\n{}\n", record1, record2))
        .expect("failed to write sidecar file");

    let results = parse_tool_results(session_id);

    // Cleanup
    let _ = std::fs::remove_file(&path);

    let placements = results
        .tactical_placements
        .expect("should have tactical_placements after parsing 2 valid records");
    assert_eq!(placements.len(), 2, "should have 2 placements");
    assert_eq!(placements[0].entity_id, "goblin-01");
    assert_eq!(placements[1].entity_id, "goblin-02");
    assert_eq!(
        placements[1].size, 2,
        "second entity should be large (size=2)"
    );
}

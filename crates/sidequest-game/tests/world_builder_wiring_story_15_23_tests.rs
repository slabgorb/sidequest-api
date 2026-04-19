//! RED-phase tests for Story 15-23: Wire WorldBuilder into session creation
//!
//! Tests the conversion of raw genre pack history data (serde_json::Value)
//! to Vec<HistoryChapter>, and the integration of WorldBuilder with session
//! creation to produce materialized GameSnapshots at target maturity levels.

use sidequest_game::world_materialization::CampaignMaturity;

// ═══════════════════════════════════════════════════════════════
// AC-1: Convert raw history JSON to Vec<HistoryChapter>
// ═══════════════════════════════════════════════════════════════

/// The genre pack loader stores history.yaml as Option<serde_json::Value>.
/// The WorldBuilder needs Vec<HistoryChapter>. We need a conversion function.
use sidequest_game::world_materialization::parse_history_chapters;

#[test]
fn parse_history_chapters_from_yaml_value() {
    let yaml = r#"
chapters:
  - id: fresh
    label: "The Beginning"
    lore:
      - "The world is new."
  - id: early
    label: "First Steps"
    lore:
      - "Factions have emerged."
    character:
      name: Kael
      race: Human
      class: Fighter
      level: 3
    npcs:
      - name: Old Maren
        role: hedge_witch
        disposition: 20
    quests:
      "Clear the cellar": "completed"
    location: Millhaven
    time_of_day: evening
"#;

    let value: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
    let chapters =
        parse_history_chapters(&value).expect("should parse history chapters from YAML value");

    assert_eq!(chapters.len(), 2);
    assert_eq!(chapters[0].id, "fresh");
    assert_eq!(chapters[1].id, "early");
    assert!(chapters[1].character.is_some());
    assert_eq!(chapters[1].npcs.len(), 1);
    assert_eq!(chapters[1].quests.len(), 1);
    assert_eq!(chapters[1].location.as_deref(), Some("Millhaven"));
}

#[test]
fn parse_history_chapters_returns_empty_on_none() {
    let value = serde_json::Value::Null;
    let chapters = parse_history_chapters(&value).expect("null should return empty vec, not error");
    assert!(chapters.is_empty());
}

#[test]
fn parse_history_chapters_returns_error_on_malformed() {
    let value = serde_json::json!({"chapters": "not an array"});
    let result = parse_history_chapters(&value);
    assert!(result.is_err(), "malformed chapters should return error");
}

#[test]
fn parse_history_chapters_handles_missing_chapters_key() {
    // history.yaml without a "chapters" key — just an empty object
    let value = serde_json::json!({});
    let chapters =
        parse_history_chapters(&value).expect("missing chapters key should return empty vec");
    assert!(chapters.is_empty());
}

// ═══════════════════════════════════════════════════════════════
// AC-2: WorldBuilder integration with genre pack data
// ═══════════════════════════════════════════════════════════════

/// Build a GameSnapshot from raw genre pack history at a target maturity.
/// This is the function the server should call during session creation.
use sidequest_game::world_materialization::materialize_from_genre_pack;

#[test]
fn materialize_from_genre_pack_at_fresh() {
    let history_yaml = r#"
chapters:
  - id: fresh
    label: "The Beginning"
    lore:
      - "The world is new."
  - id: early
    label: "First Steps"
    lore:
      - "Factions emerged."
    character:
      name: Kael
      race: Human
      class: Fighter
      level: 3
"#;
    let history_value: serde_json::Value = serde_yaml::from_str(history_yaml).unwrap();

    let snap = materialize_from_genre_pack(
        &history_value,
        CampaignMaturity::Fresh,
        "low_fantasy",
        "shattered_reach",
    )
    .expect("should materialize at Fresh");

    assert_eq!(snap.campaign_maturity, CampaignMaturity::Fresh);
    assert_eq!(snap.genre_slug, "low_fantasy");
    assert_eq!(snap.world_slug, "shattered_reach");
    // Fresh should include fresh chapter lore but NOT early chapter data
    assert!(snap
        .lore_established
        .contains(&"The world is new.".to_string()));
    assert!(!snap
        .lore_established
        .contains(&"Factions emerged.".to_string()));
    // Fresh should NOT create a character from the early chapter
    assert!(
        snap.characters.is_empty(),
        "Fresh maturity should not apply early chapter's character"
    );
}

#[test]
fn materialize_from_genre_pack_at_early_includes_character() {
    let history_yaml = r#"
chapters:
  - id: fresh
    label: "The Beginning"
    lore:
      - "The world is new."
  - id: early
    label: "First Steps"
    lore:
      - "Factions emerged."
    character:
      name: Kael
      race: Human
      class: Fighter
      level: 3
      hp: 30
      max_hp: 30
"#;
    let history_value: serde_json::Value = serde_yaml::from_str(history_yaml).unwrap();

    let snap = materialize_from_genre_pack(
        &history_value,
        CampaignMaturity::Early,
        "low_fantasy",
        "shattered_reach",
    )
    .expect("should materialize at Early");

    assert_eq!(snap.campaign_maturity, CampaignMaturity::Early);
    // Early should include both fresh and early lore
    assert_eq!(snap.lore_established.len(), 2);
    // Early should have the character from the early chapter
    assert_eq!(snap.characters.len(), 1);
    assert_eq!(snap.characters[0].core.name.as_str(), "Kael");
    assert_eq!(snap.characters[0].core.level, 3);
}

#[test]
fn materialize_from_genre_pack_at_veteran_includes_all() {
    let history_yaml = r#"
chapters:
  - id: fresh
    label: "Fresh"
    lore: ["Fresh lore."]
  - id: early
    label: "Early"
    lore: ["Early lore."]
  - id: mid
    label: "Mid"
    lore: ["Mid lore."]
  - id: veteran
    label: "Veteran"
    lore: ["Veteran lore."]
"#;
    let history_value: serde_json::Value = serde_yaml::from_str(history_yaml).unwrap();

    let snap = materialize_from_genre_pack(
        &history_value,
        CampaignMaturity::Veteran,
        "test_genre",
        "test_world",
    )
    .expect("should materialize at Veteran");

    assert_eq!(snap.lore_established.len(), 4);
    assert_eq!(snap.campaign_maturity, CampaignMaturity::Veteran);
}

#[test]
fn materialize_from_genre_pack_with_no_history() {
    let snap = materialize_from_genre_pack(
        &serde_json::Value::Null,
        CampaignMaturity::Fresh,
        "test_genre",
        "test_world",
    )
    .expect("null history should produce default snapshot");

    assert_eq!(snap.campaign_maturity, CampaignMaturity::Fresh);
    assert_eq!(snap.genre_slug, "test_genre");
    assert!(snap.lore_established.is_empty());
}

// ═══════════════════════════════════════════════════════════════
// AC-3: Materialized snapshot has genre/world slugs set
// ═══════════════════════════════════════════════════════════════

#[test]
fn materialized_snapshot_has_genre_and_world_slugs() {
    let history_yaml = r#"
chapters:
  - id: fresh
    label: "Test"
"#;
    let history_value: serde_json::Value = serde_yaml::from_str(history_yaml).unwrap();

    let snap = materialize_from_genre_pack(
        &history_value,
        CampaignMaturity::Fresh,
        "neon_dystopia",
        "franchise_nations",
    )
    .unwrap();

    assert_eq!(snap.genre_slug, "neon_dystopia");
    assert_eq!(snap.world_slug, "franchise_nations");
}

// ═══════════════════════════════════════════════════════════════
// AC-4: Full chapter data survives conversion pipeline
// ═══════════════════════════════════════════════════════════════

#[test]
fn full_chapter_data_survives_json_round_trip() {
    let history_yaml = r#"
chapters:
  - id: early
    label: "The Reluctant Sword"
    session_range: [1, 5]
    character:
      name: Kael Ashford
      race: Human
      class: Fighter
      level: 3
      hp: 30
      max_hp: 30
      ac: 14
      backstory: "A former farm hand."
      gold: 15
    npcs:
      - name: Old Maren
        role: hedge_witch
        disposition: 20
        location: "Edge of Thornfield"
      - name: Corporal Hask
        role: militia_leader
        disposition: 10
    quests:
      "Clear the cellar": "completed"
      "Scout the camp": "active"
    lore:
      - "Thornfield was a Freeholder community."
      - "Bandits wear thorn armbands."
    notes:
      - "Follow up on Pik."
    narrative_log:
      - speaker: narrator
        text: "The fire took everything."
    location: Millhaven
    time_of_day: evening
    atmosphere: "A small town."
    active_stakes: "Bandits grow bolder."
    tropes:
      - id: bandit_unification
        status: active
        progression: 0.15
"#;
    let history_value: serde_json::Value = serde_yaml::from_str(history_yaml).unwrap();

    let snap = materialize_from_genre_pack(
        &history_value,
        CampaignMaturity::Early,
        "low_fantasy",
        "shattered_reach",
    )
    .unwrap();

    // Character
    assert_eq!(snap.characters.len(), 1);
    assert_eq!(snap.characters[0].core.name.as_str(), "Kael Ashford");
    // Story 39-2: chapter hp/max_hp/ac are ignored; placeholder edge pool is
    // installed by the constructor. Story 39-3 wires per-class YAML seeding.
    assert!(snap.characters[0].core.edge.base_max > 0);

    // NPCs
    assert!(snap.npcs.len() >= 2);
    let npc_names: Vec<&str> = snap.npcs.iter().map(|n| n.core.name.as_str()).collect();
    assert!(npc_names.contains(&"Old Maren"));
    assert!(npc_names.contains(&"Corporal Hask"));

    // Quests
    assert_eq!(snap.quest_log.len(), 2);
    assert_eq!(snap.quest_log.get("Clear the cellar").unwrap(), "completed");

    // Lore
    assert_eq!(snap.lore_established.len(), 2);

    // Notes
    assert_eq!(snap.notes.len(), 1);

    // Narrative log
    assert!(!snap.narrative_log.is_empty());
    assert_eq!(snap.narrative_log[0].content, "The fire took everything.");

    // Scene context
    assert_eq!(snap.location, "Millhaven");
    assert_eq!(snap.time_of_day, "evening");
    assert_eq!(snap.atmosphere, "A small town.");
    assert_eq!(snap.active_stakes, "Bandits grow bolder.");

    // Tropes
    assert!(!snap.active_tropes.is_empty());
    assert_eq!(
        snap.active_tropes[0].trope_definition_id(),
        "bandit_unification"
    );
}

// ═══════════════════════════════════════════════════════════════
// Wiring test — materialize_from_genre_pack is importable from
// sidequest_game (public API verification)
// ═══════════════════════════════════════════════════════════════

#[test]
fn wiring_test_materialize_from_genre_pack_accessible() {
    // Verify the function is exported from the public API
    let result = materialize_from_genre_pack(
        &serde_json::Value::Null,
        CampaignMaturity::Fresh,
        "test",
        "test",
    );
    assert!(result.is_ok());
}

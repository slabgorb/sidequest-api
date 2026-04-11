//! RED-phase tests for Story 6-6: World Materialization
//!
//! Tests campaign maturity derivation, beat acceleration, history chapter
//! application to GameSnapshot, and genre pack YAML schema deserialization.

use sidequest_game::state::GameSnapshot;

// These imports will fail until the types are implemented:
use sidequest_game::world_materialization::{materialize_world, CampaignMaturity, HistoryChapter};

// ───────────────────────────────────────────────────
// AC: Maturity derivation — from_snapshot() returns correct level for turn ranges
// ───────────────────────────────────────────────────

fn snapshot_at_round(round: u32) -> GameSnapshot {
    let mut snap = GameSnapshot::default();
    // Advance TurnManager to the desired round
    for _ in 1..round {
        snap.turn_manager.advance();
    }
    snap
}

#[test]
fn maturity_fresh_at_turn_zero() {
    let snap = GameSnapshot::default();
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(maturity, CampaignMaturity::Fresh);
}

#[test]
fn maturity_fresh_at_turn_one() {
    let snap = snapshot_at_round(1);
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(maturity, CampaignMaturity::Fresh);
}

#[test]
fn maturity_fresh_boundary_at_turn_five() {
    let snap = snapshot_at_round(5);
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(maturity, CampaignMaturity::Fresh);
}

#[test]
fn maturity_early_at_turn_six() {
    let snap = snapshot_at_round(6);
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(maturity, CampaignMaturity::Early);
}

#[test]
fn maturity_early_boundary_at_turn_twenty() {
    let snap = snapshot_at_round(20);
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(maturity, CampaignMaturity::Early);
}

#[test]
fn maturity_mid_at_turn_twentyone() {
    let snap = snapshot_at_round(21);
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(maturity, CampaignMaturity::Mid);
}

#[test]
fn maturity_mid_boundary_at_turn_fifty() {
    let snap = snapshot_at_round(50);
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(maturity, CampaignMaturity::Mid);
}

#[test]
fn maturity_veteran_at_turn_fiftyone() {
    let snap = snapshot_at_round(51);
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(maturity, CampaignMaturity::Veteran);
}

#[test]
fn maturity_veteran_at_turn_two_hundred() {
    let snap = snapshot_at_round(200);
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(maturity, CampaignMaturity::Veteran);
}

// ───────────────────────────────────────────────────
// AC: Beat acceleration — beats fired contribute to effective turn count
// ───────────────────────────────────────────────────

#[test]
fn beats_accelerate_maturity_fresh_to_early() {
    // Turn 4 with 4 beats: effective = 4 + (4/2) = 6 → Early
    let mut snap = snapshot_at_round(4);
    snap.total_beats_fired = 4;
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(
        maturity,
        CampaignMaturity::Early,
        "4 beats should accelerate turn 4 past Fresh threshold"
    );
}

#[test]
fn beats_accelerate_maturity_early_to_mid() {
    // Turn 15 with 12 beats: effective = 15 + (12/2) = 21 → Mid
    let mut snap = snapshot_at_round(15);
    snap.total_beats_fired = 12;
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(
        maturity,
        CampaignMaturity::Mid,
        "12 beats should accelerate turn 15 past Early threshold"
    );
}

#[test]
fn zero_beats_no_acceleration() {
    let mut snap = snapshot_at_round(5);
    snap.total_beats_fired = 0;
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(
        maturity,
        CampaignMaturity::Fresh,
        "zero beats should not accelerate maturity"
    );
}

// ───────────────────────────────────────────────────
// AC: History application — materialize_world populates world_history
// ───────────────────────────────────────────────────

fn make_test_chapters() -> Vec<HistoryChapter> {
    vec![
        HistoryChapter {
            id: "fresh".to_string(),
            label: "The Beginning".to_string(),
            lore: vec!["The world is new.".to_string()],
            ..Default::default()
        },
        HistoryChapter {
            id: "early".to_string(),
            label: "First Steps".to_string(),
            lore: vec![
                "Factions have emerged.".to_string(),
                "Trade routes established.".to_string(),
            ],
            ..Default::default()
        },
        HistoryChapter {
            id: "mid".to_string(),
            label: "Rising Tensions".to_string(),
            lore: vec![
                "The old alliance fractures.".to_string(),
                "Border conflicts intensify.".to_string(),
                "A prophecy surfaces.".to_string(),
            ],
            ..Default::default()
        },
        HistoryChapter {
            id: "veteran".to_string(),
            label: "The Long War".to_string(),
            lore: vec![
                "The great betrayal reshaped the map.".to_string(),
                "Underground resistance movements thrive.".to_string(),
                "Ancient powers stir beneath the surface.".to_string(),
                "The final reckoning approaches.".to_string(),
            ],
            ..Default::default()
        },
    ]
}

#[test]
fn materialize_world_sets_campaign_maturity() {
    let mut snap = snapshot_at_round(25);
    let chapters = make_test_chapters();
    materialize_world(&mut snap, &chapters);
    assert_eq!(snap.campaign_maturity, CampaignMaturity::Mid);
}

#[test]
fn materialize_world_populates_world_history() {
    let mut snap = snapshot_at_round(25);
    let chapters = make_test_chapters();
    materialize_world(&mut snap, &chapters);
    assert!(
        !snap.world_history.is_empty(),
        "world_history should be populated after materialization"
    );
}

#[test]
fn materialize_world_includes_all_chapters_up_to_maturity() {
    // Mid maturity should include fresh + early + mid chapters
    let mut snap = snapshot_at_round(30);
    let chapters = make_test_chapters();
    materialize_world(&mut snap, &chapters);
    let chapter_ids: Vec<&str> = snap.world_history.iter().map(|c| c.id.as_str()).collect();
    assert!(
        chapter_ids.contains(&"fresh"),
        "should include fresh chapter"
    );
    assert!(
        chapter_ids.contains(&"early"),
        "should include early chapter"
    );
    assert!(chapter_ids.contains(&"mid"), "should include mid chapter");
    assert!(
        !chapter_ids.contains(&"veteran"),
        "should NOT include veteran chapter at Mid maturity"
    );
}

// ───────────────────────────────────────────────────
// AC: Fresh is sparse — minimal history
// ─���─────────────────────────────────────────────────

#[test]
fn fresh_yields_only_fresh_chapter() {
    let mut snap = GameSnapshot::default(); // turn 1, no beats → Fresh
    let chapters = make_test_chapters();
    materialize_world(&mut snap, &chapters);
    assert_eq!(
        snap.world_history.len(),
        1,
        "Fresh maturity should yield exactly one chapter"
    );
    assert_eq!(snap.world_history[0].id, "fresh");
}

#[test]
fn fresh_chapter_has_minimal_lore() {
    let mut snap = GameSnapshot::default();
    let chapters = make_test_chapters();
    materialize_world(&mut snap, &chapters);
    let total_lore: usize = snap.world_history.iter().map(|c| c.lore.len()).sum();
    assert!(
        total_lore <= 2,
        "Fresh should have minimal lore (1-2 lines), got {total_lore}"
    );
}

// ───────────────────────────────────────────────────
// AC: Veteran is rich — full history with faction lore
// ───────────────────────────────────────────────────

#[test]
fn veteran_yields_all_chapters() {
    let mut snap = snapshot_at_round(60);
    let chapters = make_test_chapters();
    materialize_world(&mut snap, &chapters);
    assert_eq!(
        snap.world_history.len(),
        4,
        "Veteran maturity should include all four chapters"
    );
}

#[test]
fn veteran_has_rich_lore() {
    let mut snap = snapshot_at_round(60);
    let chapters = make_test_chapters();
    materialize_world(&mut snap, &chapters);
    let total_lore: usize = snap.world_history.iter().map(|c| c.lore.len()).sum();
    assert!(
        total_lore >= 4,
        "Veteran should have rich lore across chapters, got {total_lore}"
    );
}

// ───────────────────────────────────────────────────
// AC: Idempotent — calling materialize_world twice produces same result
// ───────────────────────────────────────────────────

#[test]
fn materialize_world_is_idempotent() {
    let mut snap = snapshot_at_round(30);
    let chapters = make_test_chapters();

    materialize_world(&mut snap, &chapters);
    let first_maturity = snap.campaign_maturity.clone();
    let first_history = snap.world_history.clone();

    materialize_world(&mut snap, &chapters);
    assert_eq!(
        snap.campaign_maturity, first_maturity,
        "maturity should be identical on second call"
    );
    assert_eq!(
        snap.world_history.len(),
        first_history.len(),
        "history length should be identical on second call"
    );
}

// ───────────────────────────────────────────────────
// AC: Genre pack schema — history chapters deserialize from YAML
// ───────────────────────────────────────────────────

#[test]
fn history_chapters_deserialize_from_yaml() {
    let yaml = r#"
chapters:
  - id: fresh
    label: "The Beginning"
    lore:
      - "The world is new."
  - id: veteran
    label: "The Long War"
    lore:
      - "Ancient powers stir."
      - "The final reckoning approaches."
"#;

    let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    let chapters: Vec<HistoryChapter> = serde_yaml::from_value(parsed["chapters"].clone())
        .expect("should deserialize history chapters from YAML");

    assert_eq!(chapters.len(), 2);
    assert_eq!(chapters[0].id, "fresh");
    assert_eq!(chapters[1].id, "veteran");
    assert_eq!(chapters[1].lore.len(), 2);
}

#[test]
fn history_chapter_requires_id() {
    let yaml = r#"
- label: "No ID Chapter"
  lore:
    - "Missing ID field."
"#;
    let result: Result<Vec<HistoryChapter>, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "chapters without id should fail deserialization"
    );
}

// ───────────────────────────────────────────────────
// Rule #2: CampaignMaturity must be #[non_exhaustive]
// ───────────────────────────────────────────────────

#[test]
fn campaign_maturity_ordering() {
    // CampaignMaturity should implement Ord so maturity levels can be compared
    assert!(CampaignMaturity::Fresh < CampaignMaturity::Early);
    assert!(CampaignMaturity::Early < CampaignMaturity::Mid);
    assert!(CampaignMaturity::Mid < CampaignMaturity::Veteran);
}

#[test]
fn campaign_maturity_equality() {
    assert_eq!(CampaignMaturity::Fresh, CampaignMaturity::Fresh);
    assert_ne!(CampaignMaturity::Fresh, CampaignMaturity::Veteran);
}

#[test]
fn campaign_maturity_clone() {
    let m = CampaignMaturity::Mid;
    let cloned = m.clone();
    assert_eq!(m, cloned);
}

#[test]
fn campaign_maturity_debug_format() {
    // Verify Debug is derived (will fail to compile if not)
    let debug_str = format!("{:?}", CampaignMaturity::Fresh);
    assert!(
        debug_str.contains("Fresh"),
        "Debug should contain variant name"
    );
}

// ───────────────────────────────────────────────────
// Rule #8: Deserialize consistency — CampaignMaturity round-trips through serde
// ───────────────────────────────────────────────────

#[test]
fn campaign_maturity_serializes_to_expected_string() {
    let json = serde_json::to_string(&CampaignMaturity::Veteran).unwrap();
    // Should serialize as a string variant, not a number
    assert!(
        json.contains("Veteran") || json.contains("veteran"),
        "CampaignMaturity should serialize as string, got: {json}"
    );
}

#[test]
fn campaign_maturity_round_trips_through_json() {
    for variant in [
        CampaignMaturity::Fresh,
        CampaignMaturity::Early,
        CampaignMaturity::Mid,
        CampaignMaturity::Veteran,
    ] {
        let json = serde_json::to_string(&variant).unwrap();
        let deserialized: CampaignMaturity = serde_json::from_str(&json).unwrap();
        assert_eq!(variant, deserialized, "round-trip failed for {variant:?}");
    }
}

#[test]
fn campaign_maturity_rejects_invalid_variant() {
    let result: Result<CampaignMaturity, _> = serde_json::from_str(r#""SuperVeteran""#);
    assert!(result.is_err(), "should reject unknown maturity variant");
}

// ───────────────────────────────────────────────────
// Edge cases
// ───────────────────────────────────────────────────

#[test]
fn materialize_with_empty_chapters_leaves_empty_history() {
    let mut snap = snapshot_at_round(30);
    let chapters: Vec<HistoryChapter> = vec![];
    materialize_world(&mut snap, &chapters);
    assert!(
        snap.world_history.is_empty(),
        "empty chapters input should yield empty history"
    );
    // Maturity should still be calculated
    assert_eq!(snap.campaign_maturity, CampaignMaturity::Mid);
}

#[test]
fn materialize_with_only_veteran_chapter_at_fresh() {
    // If genre pack only has veteran chapter, Fresh gets nothing
    let mut snap = GameSnapshot::default();
    let chapters = vec![HistoryChapter {
        id: "veteran".to_string(),
        label: "Late Game".to_string(),
        lore: vec!["Deep lore.".to_string()],
        ..Default::default()
    }];
    materialize_world(&mut snap, &chapters);
    assert!(
        snap.world_history.is_empty(),
        "Fresh maturity should not include veteran-only chapters"
    );
}

#[test]
fn large_beat_count_does_not_overflow() {
    let mut snap = snapshot_at_round(10);
    snap.total_beats_fired = u32::MAX;
    // Should not panic — caps at Veteran
    let maturity = CampaignMaturity::from_snapshot(&snap);
    assert_eq!(maturity, CampaignMaturity::Veteran);
}

//! RED-phase tests for Story 18-8: Port WorldBuilder from Python
//!
//! Tests the fluent WorldBuilder API that materializes GameSnapshot at
//! different campaign maturity levels. Ported from Python's
//! `sidequest/game/world_builder.py` (~410 LOC builder pattern).
//!
//! The WorldBuilder expands the existing `materialize_world()` function
//! from story 6-6 into a full builder with chapter application logic:
//! character stats, NPCs, quests, lore, narrative log, scene context,
//! tropes, and extras (extra NPCs, extra lore, combat setup).

use sidequest_game::world_materialization::{CampaignMaturity, HistoryChapter, WorldBuilder};

// ═══════════════════════════════════════════════════════════════
// AC-1: WorldBuilder fluent API — construct and configure builder
// ═══════════════════════════════════════════════════════════════

#[test]
fn world_builder_new_defaults_to_fresh() {
    let builder = WorldBuilder::new();
    let snap = builder.build();
    assert_eq!(snap.campaign_maturity, CampaignMaturity::Fresh);
}

#[test]
fn world_builder_at_maturity_sets_level() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Veteran)
        .build();
    assert_eq!(snap.campaign_maturity, CampaignMaturity::Veteran);
}

#[test]
fn world_builder_fluent_chain() {
    // All builder methods return Self for chaining
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Mid)
        .with_extra_npcs(3)
        .with_extra_lore(5)
        .build();

    // Should produce a valid snapshot — specific content tested below
    assert_eq!(snap.campaign_maturity, CampaignMaturity::Mid);
}

// ═══════════════════════════════════════════════════════════════
// AC-2: HistoryChapter expansion — full chapter data structure
// ═══════════════════════════════════════════════════════════════

#[test]
fn history_chapter_has_character_field() {
    let chapter = HistoryChapter {
        id: "early".to_string(),
        label: "The Beginning".to_string(),
        lore: vec![],
        character: Some(ChapterCharacter {
            name: "Kael".to_string(),
            race: "Human".to_string(),
            class: "Fighter".to_string(),
            level: 3,
            hp: Some(20),
            max_hp: Some(20),
            ac: Some(10),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(chapter.character.is_some());
    assert_eq!(chapter.character.unwrap().name, "Kael");
}

#[test]
fn history_chapter_has_npcs_field() {
    let chapter = HistoryChapter {
        id: "early".to_string(),
        label: "Test".to_string(),
        lore: vec![],
        npcs: vec![ChapterNpc {
            name: "Old Maren".to_string(),
            role: Some("hedge_witch".to_string()),
            disposition: Some(20),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(chapter.npcs.len(), 1);
    assert_eq!(chapter.npcs[0].name, "Old Maren");
}

#[test]
fn history_chapter_has_quests_field() {
    let chapter = HistoryChapter {
        id: "early".to_string(),
        label: "Test".to_string(),
        lore: vec![],
        quests: vec![("Clear the cellar".to_string(), "completed".to_string())]
            .into_iter()
            .collect(),
        ..Default::default()
    };
    assert_eq!(chapter.quests.len(), 1);
    assert_eq!(chapter.quests.get("Clear the cellar").unwrap(), "completed");
}

#[test]
fn history_chapter_has_scene_context_fields() {
    let chapter = HistoryChapter {
        id: "early".to_string(),
        label: "Test".to_string(),
        lore: vec![],
        location: Some("Millhaven".to_string()),
        time_of_day: Some("evening".to_string()),
        atmosphere: Some("A quiet town.".to_string()),
        active_stakes: Some("Bandits are coming.".to_string()),
        ..Default::default()
    };
    assert_eq!(chapter.location.as_deref(), Some("Millhaven"));
    assert_eq!(chapter.time_of_day.as_deref(), Some("evening"));
    assert_eq!(chapter.atmosphere.as_deref(), Some("A quiet town."));
    assert_eq!(
        chapter.active_stakes.as_deref(),
        Some("Bandits are coming.")
    );
}

#[test]
fn history_chapter_has_narrative_log() {
    let chapter = HistoryChapter {
        id: "early".to_string(),
        label: "Test".to_string(),
        lore: vec![],
        narrative_log: vec![ChapterNarrativeEntry {
            speaker: "narrator".to_string(),
            text: "The fire took everything.".to_string(),
        }],
        ..Default::default()
    };
    assert_eq!(chapter.narrative_log.len(), 1);
    assert_eq!(chapter.narrative_log[0].speaker, "narrator");
}

#[test]
fn history_chapter_has_notes() {
    let chapter = HistoryChapter {
        id: "early".to_string(),
        label: "Test".to_string(),
        lore: vec![],
        notes: vec!["Follow up on Pik's lead.".to_string()],
        ..Default::default()
    };
    assert_eq!(chapter.notes.len(), 1);
}

#[test]
fn history_chapter_has_tropes() {
    let chapter = HistoryChapter {
        id: "early".to_string(),
        label: "Test".to_string(),
        lore: vec![],
        tropes: vec![ChapterTrope {
            id: "bandit_unification".to_string(),
            status: "active".to_string(),
            progression: 0.15,
            notes: vec![],
        }],
        ..Default::default()
    };
    assert_eq!(chapter.tropes.len(), 1);
    assert_eq!(chapter.tropes[0].id, "bandit_unification");
}

// Use the expanded chapter types from world_materialization
use sidequest_game::world_materialization::{
    ChapterCharacter, ChapterNarrativeEntry, ChapterNpc, ChapterTrope,
};

// ═══════════════════════════════════════════════════════════════
// AC-3: Chapter application — character creation/update
// ═══════════════════════════════════════════════════════════════

fn make_chapter_with_character() -> HistoryChapter {
    HistoryChapter {
        id: "early".to_string(),
        label: "The Reluctant Sword".to_string(),
        lore: vec!["The world is new.".to_string()],
        character: Some(ChapterCharacter {
            name: "Kael Ashford".to_string(),
            race: "Human".to_string(),
            class: "Fighter".to_string(),
            level: 3,
            hp: Some(20),
            max_hp: Some(20),
            ac: Some(10),
            backstory: Some("A former farm hand.".to_string()),
            personality: Some("Dry-witted, slow to trust.".to_string()),
            description: Some("A broad-shouldered young man.".to_string()),
            gold: Some(15),
        }),
        ..Default::default()
    }
}

#[test]
fn chapter_application_creates_character() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Early)
        .with_chapters(vec![make_chapter_with_character()])
        .build();

    assert!(!snap.characters.is_empty(), "character should be created");
    let char = &snap.characters[0];
    assert_eq!(char.core.name.as_str(), "Kael Ashford");
    assert_eq!(char.race.as_str(), "Human");
    assert_eq!(char.char_class.as_str(), "Fighter");
    assert_eq!(char.core.level, 3);
}

#[test]
fn chapter_application_sets_character_stats() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Early)
        .with_chapters(vec![make_chapter_with_character()])
        .build();

    let char = &snap.characters[0];
    // Story 39-2: ChapterCharacter hp/max_hp values are advisory; the
    // placeholder edge pool is synthesized regardless (39-3 tunes).
    assert!(char.core.edge.base_max > 0);
}

#[test]
fn second_chapter_updates_existing_character() {
    let early = HistoryChapter {
        id: "early".to_string(),
        label: "Early".to_string(),
        character: Some(ChapterCharacter {
            name: "Kael".to_string(),
            race: "Human".to_string(),
            class: "Fighter".to_string(),
            level: 3,
            hp: Some(20),
            max_hp: Some(20),
            ac: Some(10),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mid = HistoryChapter {
        id: "mid".to_string(),
        label: "Mid".to_string(),
        character: Some(ChapterCharacter {
            level: 7,
            hp: Some(20),
            max_hp: Some(20),
            ac: Some(10),
            ..Default::default()
        }),
        ..Default::default()
    };

    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Mid)
        .with_chapters(vec![early, mid])
        .build();

    assert_eq!(snap.characters.len(), 1, "should update, not duplicate");
    let char = &snap.characters[0];
    assert_eq!(
        char.core.level, 7,
        "level should be updated to mid chapter value"
    );
    // Story 39-2: chapter hp updates are advisory; edge pool is placeholder.
    assert!(char.core.edge.base_max > 0);
}

// ═══════════════════════════════════════════════════════════════
// AC-4: Chapter application — NPC creation
// ═══════════════════════════════════════════════════════════════

fn make_chapter_with_npcs() -> HistoryChapter {
    HistoryChapter {
        id: "early".to_string(),
        label: "Test".to_string(),
        lore: vec![],
        npcs: vec![
            ChapterNpc {
                name: "Old Maren".to_string(),
                role: Some("hedge_witch".to_string()),
                description: Some("A gruff hedge witch.".to_string()),
                personality: Some("Maternal in a terrifying way.".to_string()),
                disposition: Some(20),
                location: Some("Edge of Thornfield".to_string()),
                ..Default::default()
            },
            ChapterNpc {
                name: "Corporal Hask".to_string(),
                role: Some("militia_leader".to_string()),
                description: Some("A tired middle-aged man.".to_string()),
                disposition: Some(10),
                location: Some("Millhaven barracks".to_string()),
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

#[test]
fn chapter_application_creates_npcs() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Early)
        .with_chapters(vec![make_chapter_with_npcs()])
        .build();

    assert!(
        snap.npcs.len() >= 2,
        "should have at least 2 NPCs, got {}",
        snap.npcs.len()
    );
    let names: Vec<&str> = snap.npcs.iter().map(|n| n.core.name.as_str()).collect();
    assert!(names.contains(&"Old Maren"), "should contain Old Maren");
    assert!(
        names.contains(&"Corporal Hask"),
        "should contain Corporal Hask"
    );
}

#[test]
fn chapter_npc_updates_existing_npc() {
    let early = HistoryChapter {
        id: "early".to_string(),
        label: "Early".to_string(),
        npcs: vec![ChapterNpc {
            name: "Old Maren".to_string(),
            disposition: Some(20),
            location: Some("Thornfield".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };
    let mid = HistoryChapter {
        id: "mid".to_string(),
        label: "Mid".to_string(),
        npcs: vec![ChapterNpc {
            name: "Old Maren".to_string(),
            disposition: Some(30),
            location: Some("Player's camp".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Mid)
        .with_chapters(vec![early, mid])
        .build();

    let maren_count = snap
        .npcs
        .iter()
        .filter(|n| n.core.name.as_str() == "Old Maren")
        .count();
    assert_eq!(maren_count, 1, "should update existing NPC, not duplicate");
}

// ═══════════════════════════════════════════════════════════════
// AC-5: Chapter application — quests, lore, notes, narrative log
// ═══════════════════════════════════════════════════════════════

fn make_chapter_with_world_data() -> HistoryChapter {
    HistoryChapter {
        id: "early".to_string(),
        label: "Early".to_string(),
        lore: vec![
            "Thornfield was a Freeholder community.".to_string(),
            "The cellar was infested by rats.".to_string(),
        ],
        quests: vec![
            ("Clear the cellar".to_string(), "completed".to_string()),
            ("Scout the bandit camp".to_string(), "active".to_string()),
        ]
        .into_iter()
        .collect(),
        notes: vec!["Pik may have overheard something.".to_string()],
        narrative_log: vec![ChapterNarrativeEntry {
            speaker: "narrator".to_string(),
            text: "The fire took everything.".to_string(),
        }],
        ..Default::default()
    }
}

#[test]
fn chapter_application_populates_lore() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Early)
        .with_chapters(vec![make_chapter_with_world_data()])
        .build();

    assert_eq!(snap.lore_established.len(), 2);
    assert!(snap
        .lore_established
        .contains(&"Thornfield was a Freeholder community.".to_string()));
}

#[test]
fn chapter_application_deduplicates_lore() {
    let ch1 = HistoryChapter {
        id: "early".to_string(),
        label: "Early".to_string(),
        lore: vec!["Shared lore.".to_string()],
        ..Default::default()
    };
    let ch2 = HistoryChapter {
        id: "mid".to_string(),
        label: "Mid".to_string(),
        lore: vec!["Shared lore.".to_string(), "New mid lore.".to_string()],
        ..Default::default()
    };

    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Mid)
        .with_chapters(vec![ch1, ch2])
        .build();

    let shared_count = snap
        .lore_established
        .iter()
        .filter(|l| *l == "Shared lore.")
        .count();
    assert_eq!(shared_count, 1, "duplicate lore should be deduplicated");
    assert_eq!(
        snap.lore_established.len(),
        2,
        "should have 2 unique lore entries"
    );
}

#[test]
fn chapter_application_populates_quest_log() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Early)
        .with_chapters(vec![make_chapter_with_world_data()])
        .build();

    assert_eq!(snap.quest_log.len(), 2);
    assert_eq!(snap.quest_log.get("Clear the cellar").unwrap(), "completed");
    assert_eq!(
        snap.quest_log.get("Scout the bandit camp").unwrap(),
        "active"
    );
}

#[test]
fn chapter_application_populates_notes() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Early)
        .with_chapters(vec![make_chapter_with_world_data()])
        .build();

    assert_eq!(snap.notes.len(), 1);
    assert_eq!(snap.notes[0], "Pik may have overheard something.");
}

#[test]
fn chapter_application_populates_narrative_log() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Early)
        .with_chapters(vec![make_chapter_with_world_data()])
        .build();

    assert!(
        !snap.narrative_log.is_empty(),
        "narrative log should have entries"
    );
    assert_eq!(snap.narrative_log[0].content, "The fire took everything.");
}

// ═══════════════════════════════════════════════════════════════
// AC-6: Chapter application — scene context overwrites
// ═══════════════════════════════════════════════════════════════

#[test]
fn chapter_application_sets_scene_context() {
    let chapter = HistoryChapter {
        id: "early".to_string(),
        label: "Test".to_string(),
        lore: vec![],
        location: Some("Millhaven".to_string()),
        time_of_day: Some("evening".to_string()),
        atmosphere: Some("A small town trying to hold together.".to_string()),
        active_stakes: Some("Bandits grow bolder.".to_string()),
        ..Default::default()
    };

    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Early)
        .with_chapters(vec![chapter])
        .build();

    assert_eq!(snap.location, "Millhaven");
    assert_eq!(snap.time_of_day, "evening");
    assert_eq!(snap.atmosphere, "A small town trying to hold together.");
    assert_eq!(snap.active_stakes, "Bandits grow bolder.");
}

#[test]
fn later_chapter_overwrites_scene_context() {
    let early = HistoryChapter {
        id: "early".to_string(),
        label: "Early".to_string(),
        location: Some("Millhaven".to_string()),
        time_of_day: Some("evening".to_string()),
        ..Default::default()
    };
    let mid = HistoryChapter {
        id: "mid".to_string(),
        label: "Mid".to_string(),
        location: Some("The Ashen Citadel".to_string()),
        time_of_day: Some("dawn".to_string()),
        ..Default::default()
    };

    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Mid)
        .with_chapters(vec![early, mid])
        .build();

    assert_eq!(
        snap.location, "The Ashen Citadel",
        "later chapter should overwrite location"
    );
    assert_eq!(
        snap.time_of_day, "dawn",
        "later chapter should overwrite time_of_day"
    );
}

// ═══════════════════════════════════════════════════════════════
// AC-7: Chapter application — trope state
// ═══════════════════════════════════════════════════════════════

#[test]
fn chapter_application_creates_trope_state() {
    let chapter = HistoryChapter {
        id: "early".to_string(),
        label: "Test".to_string(),
        lore: vec![],
        tropes: vec![ChapterTrope {
            id: "bandit_unification".to_string(),
            status: "active".to_string(),
            progression: 0.15,
            notes: vec!["Bandits wear thorn armbands.".to_string()],
        }],
        ..Default::default()
    };

    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Early)
        .with_chapters(vec![chapter])
        .build();

    assert!(!snap.active_tropes.is_empty(), "should have active tropes");
    let trope = &snap.active_tropes[0];
    assert_eq!(trope.trope_definition_id(), "bandit_unification");
    assert!((trope.progression() - 0.15).abs() < f64::EPSILON);
}

#[test]
fn chapter_application_updates_existing_trope() {
    let early = HistoryChapter {
        id: "early".to_string(),
        label: "Early".to_string(),
        tropes: vec![ChapterTrope {
            id: "bandit_unification".to_string(),
            status: "active".to_string(),
            progression: 0.15,
            notes: vec![],
        }],
        ..Default::default()
    };
    let mid = HistoryChapter {
        id: "mid".to_string(),
        label: "Mid".to_string(),
        tropes: vec![ChapterTrope {
            id: "bandit_unification".to_string(),
            status: "progressing".to_string(),
            progression: 0.55,
            notes: vec!["The Thorn Queen revealed.".to_string()],
        }],
        ..Default::default()
    };

    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Mid)
        .with_chapters(vec![early, mid])
        .build();

    let bandit_tropes: Vec<_> = snap
        .active_tropes
        .iter()
        .filter(|t| t.trope_definition_id() == "bandit_unification")
        .collect();
    assert_eq!(bandit_tropes.len(), 1, "should update, not duplicate trope");
    assert!(
        (bandit_tropes[0].progression() - 0.55).abs() < f64::EPSILON,
        "progression should be updated to mid chapter value"
    );
}

// ═══════════════════════════════════════════════════════════════
// AC-8: Cumulative chapter application — maturity filtering
// ═══════════════════════════════════════════════════════════════

fn make_full_chapter_set() -> Vec<HistoryChapter> {
    vec![
        HistoryChapter {
            id: "fresh".to_string(),
            label: "Fresh".to_string(),
            lore: vec!["Fresh lore.".to_string()],
            ..Default::default()
        },
        HistoryChapter {
            id: "early".to_string(),
            label: "Early".to_string(),
            lore: vec!["Early lore.".to_string()],
            ..Default::default()
        },
        HistoryChapter {
            id: "mid".to_string(),
            label: "Mid".to_string(),
            lore: vec!["Mid lore.".to_string()],
            ..Default::default()
        },
        HistoryChapter {
            id: "veteran".to_string(),
            label: "Veteran".to_string(),
            lore: vec!["Veteran lore.".to_string()],
            ..Default::default()
        },
    ]
}

#[test]
fn early_maturity_includes_fresh_and_early_chapters() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Early)
        .with_chapters(make_full_chapter_set())
        .build();

    assert!(snap.lore_established.contains(&"Fresh lore.".to_string()));
    assert!(snap.lore_established.contains(&"Early lore.".to_string()));
    assert!(!snap.lore_established.contains(&"Mid lore.".to_string()));
    assert!(!snap.lore_established.contains(&"Veteran lore.".to_string()));
}

#[test]
fn mid_maturity_includes_fresh_early_and_mid() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Mid)
        .with_chapters(make_full_chapter_set())
        .build();

    assert!(snap.lore_established.contains(&"Fresh lore.".to_string()));
    assert!(snap.lore_established.contains(&"Early lore.".to_string()));
    assert!(snap.lore_established.contains(&"Mid lore.".to_string()));
    assert!(!snap.lore_established.contains(&"Veteran lore.".to_string()));
}

#[test]
fn veteran_maturity_includes_all_chapters() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Veteran)
        .with_chapters(make_full_chapter_set())
        .build();

    assert_eq!(snap.lore_established.len(), 4);
}

#[test]
fn fresh_maturity_includes_only_fresh() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Fresh)
        .with_chapters(make_full_chapter_set())
        .build();

    assert!(snap.lore_established.contains(&"Fresh lore.".to_string()));
    assert!(!snap.lore_established.contains(&"Early lore.".to_string()));
}

// ═══════════════════════════════════════════════════════════════
// AC-9: Extra NPCs
// ═══════════════════════════════════════════════════════════════

#[test]
fn with_extra_npcs_adds_generated_npcs() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Fresh)
        .with_extra_npcs(5)
        .build();

    assert!(
        snap.npcs.len() >= 5,
        "should have at least 5 extra NPCs, got {}",
        snap.npcs.len()
    );
}

#[test]
fn extra_npcs_have_unique_names() {
    let snap = WorldBuilder::new().with_extra_npcs(10).build();

    let names: Vec<&str> = snap.npcs.iter().map(|n| n.core.name.as_str()).collect();
    let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
    assert_eq!(
        names.len(),
        unique.len(),
        "extra NPC names should be unique"
    );
}

// ═══════════════════════════════════════════════════════════════
// AC-10: Extra lore
// ═══════════════════════════════════════════════════════════════

#[test]
fn with_extra_lore_adds_generated_entries() {
    let snap = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Fresh)
        .with_extra_lore(3)
        .build();

    assert!(
        snap.lore_established.len() >= 3,
        "should have at least 3 lore entries, got {}",
        snap.lore_established.len()
    );
}

#[test]
fn extra_lore_is_deduplicated() {
    // Even with extra lore, no duplicates should appear
    let snap = WorldBuilder::new().with_extra_lore(5).build();

    let unique: std::collections::HashSet<&str> =
        snap.lore_established.iter().map(|l| l.as_str()).collect();
    assert_eq!(
        snap.lore_established.len(),
        unique.len(),
        "extra lore should not create duplicates"
    );
}

// ═══════════════════════════════════════════════════════════════
// AC-11: Combat setup — removed; GameSnapshot.combat was replaced
// with encounter: Option<StructuredEncounter> in story 16-2.
// WorldBuilder::with_combat coverage needs a new test against
// StructuredEncounter (followup story).
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// AC-12: Build is idempotent / repeatable
// ═══════════════════════════════════════════════════════════════

#[test]
fn build_is_repeatable() {
    let builder = WorldBuilder::new()
        .at_maturity(CampaignMaturity::Mid)
        .with_chapters(make_full_chapter_set())
        .with_extra_npcs(2)
        .with_extra_lore(3);

    let snap1 = builder.build();
    let snap2 = builder.build();

    assert_eq!(snap1.campaign_maturity, snap2.campaign_maturity);
    assert_eq!(snap1.lore_established.len(), snap2.lore_established.len());
    assert_eq!(snap1.npcs.len(), snap2.npcs.len());
}

// ═══════════════════════════════════════════════════════════════
// AC-13: HistoryChapter YAML deserialization — expanded schema
// ═══════════════════════════════════════════════════════════════

#[test]
fn expanded_history_chapter_deserializes_from_yaml() {
    let yaml = r#"
id: early
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
quests:
  "Clear the cellar": "completed"
  "Scout the camp": "active"
lore:
  - "Thornfield was a Freeholder community."
notes:
  - "Follow up on Pik's lead."
narrative_log:
  - speaker: narrator
    text: "The fire took everything."
location: Millhaven
time_of_day: evening
atmosphere: "A small town trying to hold together."
active_stakes: "Bandits grow bolder."
tropes:
  - id: bandit_unification
    status: active
    progression: 0.15
"#;

    let chapter: HistoryChapter =
        serde_yaml::from_str(yaml).expect("expanded HistoryChapter should deserialize from YAML");

    assert_eq!(chapter.id, "early");
    assert!(chapter.character.is_some());
    assert_eq!(chapter.character.as_ref().unwrap().name, "Kael Ashford");
    assert_eq!(chapter.npcs.len(), 1);
    assert_eq!(chapter.quests.len(), 2);
    assert_eq!(chapter.lore.len(), 1);
    assert_eq!(chapter.notes.len(), 1);
    assert_eq!(chapter.narrative_log.len(), 1);
    assert_eq!(chapter.location.as_deref(), Some("Millhaven"));
    assert_eq!(chapter.tropes.len(), 1);
}

#[test]
fn history_chapter_with_minimal_fields_deserializes() {
    // Only id and label are required; everything else should default
    let yaml = r#"
id: fresh
label: "The Beginning"
"#;

    let chapter: HistoryChapter =
        serde_yaml::from_str(yaml).expect("minimal HistoryChapter should deserialize");

    assert_eq!(chapter.id, "fresh");
    assert_eq!(chapter.label, "The Beginning");
    assert!(chapter.character.is_none());
    assert!(chapter.npcs.is_empty());
    assert!(chapter.quests.is_empty());
    assert!(chapter.lore.is_empty());
}

// ═══════════════════════════════════════════════════════════════
// Wiring test — WorldBuilder is importable and usable from test
// (integration test verifying the public API surface)
// ═══════════════════════════════════════════════════════════════

#[test]
fn world_builder_public_api_accessible() {
    // Verify the WorldBuilder and all chapter types are exported
    // from sidequest_game::world_materialization
    let _builder = WorldBuilder::new();
    let _maturity = CampaignMaturity::Fresh;
    let _chapter = HistoryChapter {
        id: "test".to_string(),
        label: "test".to_string(),
        ..Default::default()
    };
    let _char = ChapterCharacter::default();
    let _npc = ChapterNpc::default();
    let _entry = ChapterNarrativeEntry {
        speaker: "test".to_string(),
        text: "test".to_string(),
    };
    let _trope = ChapterTrope {
        id: "test".to_string(),
        status: "active".to_string(),
        progression: 0.0,
        notes: vec![],
    };
}

// ═══════════════════════════════════════════════════════════════
// Rule #2: CampaignMaturity already has #[non_exhaustive] — verified
// Rule #6: Self-check — all tests above have meaningful assertions
// Rule #8: HistoryChapter serde — tested via YAML deser tests above
// ═══════════════════════════════════════════════════════════════

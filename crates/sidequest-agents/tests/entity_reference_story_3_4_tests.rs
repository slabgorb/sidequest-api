//! Story 3-4 RED: Entity reference validation — narration mentions checked against GameSnapshot.
//!
//! Tests that the entity reference validator correctly:
//!   1. Builds an EntityRegistry from snapshot (characters, NPCs, items, locations, regions)
//!   2. Extracts capitalized phrases as potential entity references
//!   3. Passes narration referencing known entities (no warnings)
//!   4. Flags narration referencing unknown entities (warnings)
//!   5. Handles compound names via substring matching ("Old Grimjaw" → "Grimjaw")
//!   6. Skips sentence-initial capitalization and stop words
//!   7. Emits tracing::warn! with component="watcher", check="entity_reference"
//!
//! RED state: All stubs return empty Vecs / false, so every assertion expecting
//! warnings or matches will fail. The Dev agent implements GREEN.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

use sidequest_agents::agents::intent_router::Intent;
use sidequest_agents::entity_reference::{
    check_entity_references, extract_potential_references, EntityRegistry,
};
use sidequest_agents::patch_legality::ValidationResult;
use sidequest_agents::turn_record::{PatchSummary, TurnRecord};
use sidequest_game::{
    Character, CombatState, CreatureCore, Disposition, GameSnapshot, Inventory, Item, Npc,
    StateDelta, TurnManager,
};
use sidequest_protocol::NonBlankString;

// ===========================================================================
// Test infrastructure: mock builders
// ===========================================================================

/// Build a minimal GameSnapshot for testing.
fn mock_game_snapshot() -> GameSnapshot {
    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        characters: vec![],
        npcs: vec![],
        location: "The Rusty Valve".to_string(),
        time_of_day: "dusk".to_string(),
        quest_log: HashMap::new(),
        notes: vec![],
        narrative_log: vec![],
        combat: CombatState::new(),
        chase: None,
        active_tropes: vec![],
        atmosphere: "tense and electric".to_string(),
        current_region: "flickering_reach".to_string(),
        discovered_regions: vec!["flickering_reach".to_string()],
        discovered_routes: vec![],
        turn_manager: TurnManager::new(),
        last_saved_at: None,
    }
}

/// Build a mock StateDelta (all fields private, must go through serde).
fn mock_state_delta() -> StateDelta {
    serde_json::from_value(serde_json::json!({
        "characters": false,
        "npcs": false,
        "location": false,
        "time_of_day": false,
        "quest_log": false,
        "notes": false,
        "combat": false,
        "chase": false,
        "tropes": false,
        "atmosphere": false,
        "regions": false,
        "routes": false,
        "new_location": null
    }))
    .expect("mock StateDelta should deserialize")
}

/// Build an NPC with the given name.
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
            inventory: Inventory::default(),
            statuses: vec![],
        },
        voice_id: None,
        disposition: Disposition::new(0),
        location: Some(NonBlankString::new("The Rusty Valve").unwrap()),
    }
}

/// Build a Character with the given name and optional inventory items.
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
            inventory: Inventory {
                items,
                gold: 100,
            },
            statuses: vec![],
        },
        backstory: NonBlankString::new("A hero on a quest").unwrap(),
        narrative_state: "exploring".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        stats: HashMap::new(),
    }
}

/// Build a TurnRecord with the given narration and customizable snapshot_after.
fn make_record_with_narration(narration: &str) -> TurnRecord {
    TurnRecord {
        turn_id: 1,
        timestamp: Utc::now(),
        player_input: "test action".to_string(),
        classified_intent: Intent::Exploration,
        agent_name: "narrator".to_string(),
        narration: narration.to_string(),
        patches_applied: vec![PatchSummary {
            patch_type: "world".to_string(),
            fields_changed: vec!["notes".to_string()],
        }],
        snapshot_before: mock_game_snapshot(),
        snapshot_after: mock_game_snapshot(),
        delta: mock_state_delta(),
        beats_fired: vec![],
        extraction_tier: 1,
        token_count_in: 500,
        token_count_out: 100,
        agent_duration_ms: 1200,
        is_degraded: false,
    }
}

// ===========================================================================
// Tracing capture infrastructure (reused from story 3-3)
// ===========================================================================

#[derive(Debug, Clone)]
struct CapturedEvent {
    fields: Vec<(String, String)>,
    level: tracing::Level,
}

impl CapturedEvent {
    fn field_value(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

struct EventCaptureLayer {
    captured: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl EventCaptureLayer {
    fn new() -> (Self, Arc<Mutex<Vec<CapturedEvent>>>) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                captured: captured.clone(),
            },
            captured,
        )
    }
}

impl<S: Subscriber> tracing_subscriber::Layer<S> for EventCaptureLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = Vec::new();
        let mut visitor = EventFieldVisitor(&mut fields);
        event.record(&mut visitor);

        self.captured.lock().unwrap().push(CapturedEvent {
            fields,
            level: *event.metadata().level(),
        });
    }
}

struct EventFieldVisitor<'a>(&'a mut Vec<(String, String)>);

impl<'a> tracing::field::Visit for EventFieldVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0
            .push((field.name().to_string(), format!("{:?}", value)));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
}

/// Helper: find captured WARN events with component="watcher" and check="entity_reference".
fn entity_ref_warnings(events: &[CapturedEvent]) -> Vec<&CapturedEvent> {
    events
        .iter()
        .filter(|e| {
            e.level == tracing::Level::WARN
                && e.field_value("component") == Some("watcher")
                && e.field_value("check") == Some("entity_reference")
        })
        .collect()
}

// ===========================================================================
// EntityRegistry tests
// ===========================================================================

#[test]
fn registry_from_snapshot_extracts_character_names() {
    let mut snapshot = mock_game_snapshot();
    snapshot.characters = vec![make_character("Kael", vec![])];

    let registry = EntityRegistry::from_snapshot(&snapshot);

    assert!(
        registry.character_names.contains("Kael"),
        "Registry should contain character name 'Kael'"
    );
}

#[test]
fn registry_from_snapshot_extracts_npc_names() {
    let mut snapshot = mock_game_snapshot();
    snapshot.npcs = vec![make_npc("Grimjaw")];

    let registry = EntityRegistry::from_snapshot(&snapshot);

    assert!(
        registry.npc_names.contains("Grimjaw"),
        "Registry should contain NPC name 'Grimjaw'"
    );
}

#[test]
fn registry_from_snapshot_extracts_item_names() {
    let mut snapshot = mock_game_snapshot();
    snapshot.characters = vec![make_character("Kael", vec!["Flaming Sword"])];

    let registry = EntityRegistry::from_snapshot(&snapshot);

    assert!(
        registry.item_names.contains("Flaming Sword"),
        "Registry should contain item name 'Flaming Sword'"
    );
}

#[test]
fn registry_from_snapshot_extracts_location() {
    let snapshot = mock_game_snapshot(); // location = "The Rusty Valve"

    let registry = EntityRegistry::from_snapshot(&snapshot);

    assert!(
        registry.location_names.contains("The Rusty Valve"),
        "Registry should contain location 'The Rusty Valve'"
    );
}

#[test]
fn registry_from_snapshot_extracts_discovered_regions() {
    let mut snapshot = mock_game_snapshot();
    snapshot.discovered_regions = vec![
        "flickering_reach".to_string(),
        "ashen_wastes".to_string(),
    ];

    let registry = EntityRegistry::from_snapshot(&snapshot);

    assert!(
        registry.region_names.contains("flickering_reach"),
        "Registry should contain region 'flickering_reach'"
    );
    assert!(
        registry.region_names.contains("ashen_wastes"),
        "Registry should contain region 'ashen_wastes'"
    );
}

// ===========================================================================
// EntityRegistry::matches tests
// ===========================================================================

#[test]
fn matches_exact_name_case_insensitive() {
    let mut snapshot = mock_game_snapshot();
    snapshot.npcs = vec![make_npc("Grimjaw")];
    let registry = EntityRegistry::from_snapshot(&snapshot);

    assert!(
        registry.matches("Grimjaw"),
        "Exact match should return true"
    );
    assert!(
        registry.matches("grimjaw"),
        "Case-insensitive match should return true"
    );
    assert!(
        registry.matches("GRIMJAW"),
        "Case-insensitive match should return true"
    );
}

#[test]
fn matches_substring_known_name_contained_in_candidate() {
    let mut snapshot = mock_game_snapshot();
    snapshot.npcs = vec![make_npc("Grimjaw")];
    let registry = EntityRegistry::from_snapshot(&snapshot);

    // "Old Grimjaw" contains known name "Grimjaw" → should match
    assert!(
        registry.matches("Old Grimjaw"),
        "Candidate containing known name as substring should match"
    );
}

#[test]
fn matches_substring_candidate_contained_in_known_name() {
    let mut snapshot = mock_game_snapshot();
    snapshot.characters = vec![make_character("Kael Stormborn", vec![])];
    let registry = EntityRegistry::from_snapshot(&snapshot);

    // "Kael" is a substring of known name "Kael Stormborn" → should match
    assert!(
        registry.matches("Kael"),
        "Candidate that is substring of known name should match"
    );
}

#[test]
fn matches_returns_false_for_unknown_entity() {
    let mut snapshot = mock_game_snapshot();
    snapshot.npcs = vec![make_npc("Grimjaw")];
    let registry = EntityRegistry::from_snapshot(&snapshot);

    assert!(
        !registry.matches("Mordecai"),
        "Unknown entity should not match"
    );
}

// ===========================================================================
// extract_potential_references tests
// ===========================================================================

#[test]
fn extracts_capitalized_word_mid_sentence() {
    let refs = extract_potential_references("The warrior spotted Grimjaw across the room.");
    assert!(
        refs.contains(&"Grimjaw".to_string()),
        "Should extract capitalized word 'Grimjaw' from mid-sentence; got: {:?}",
        refs
    );
}

#[test]
fn extracts_multi_word_capitalized_phrase() {
    let refs = extract_potential_references("The blade known as Flaming Sword glowed brightly.");
    assert!(
        refs.iter().any(|r| r.contains("Flaming Sword") || (r.contains("Flaming") && refs.contains(&"Sword".to_string()))),
        "Should extract multi-word capitalized phrase 'Flaming Sword'; got: {:?}",
        refs
    );
}

#[test]
fn skips_sentence_initial_capitalization() {
    let refs = extract_potential_references("Darkness falls over the land.");
    assert!(
        !refs.contains(&"Darkness".to_string()),
        "Should skip sentence-initial 'Darkness'; got: {:?}",
        refs
    );
}

#[test]
fn skips_sentence_initial_after_period() {
    let refs = extract_potential_references("The night is cold. Shadows creep along the walls.");
    assert!(
        !refs.contains(&"Shadows".to_string()),
        "Should skip sentence-initial 'Shadows' after period; got: {:?}",
        refs
    );
}

#[test]
fn skips_stop_words() {
    // "The", "His", "With" are stop words — should not appear in results
    let refs = extract_potential_references("He raised His sword. With a cry, he charged.");
    let has_stop_word = refs.iter().any(|r| r == "His" || r == "With");
    assert!(
        !has_stop_word,
        "Should filter stop words; got: {:?}",
        refs
    );
}

#[test]
fn empty_narration_returns_no_references() {
    let refs = extract_potential_references("");
    assert!(
        refs.is_empty(),
        "Empty narration should produce no references; got: {:?}",
        refs
    );
}

#[test]
fn no_capitalized_words_returns_no_references() {
    let refs = extract_potential_references("the warrior crept through the shadows.");
    assert!(
        refs.is_empty(),
        "All-lowercase narration should produce no references; got: {:?}",
        refs
    );
}

// ===========================================================================
// check_entity_references integration tests
// ===========================================================================

#[test]
fn known_character_not_flagged() {
    let mut record = make_record_with_narration(
        "The party watches as Kael draws his sword and charges forward.",
    );
    record.snapshot_after.characters = vec![make_character("Kael", vec![])];

    let results = check_entity_references(&record);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(msg) if msg.contains("Kael")))
        .collect();
    assert!(
        warnings.is_empty(),
        "Known character 'Kael' should not be flagged; got: {:?}",
        results
    );
}

#[test]
fn unknown_entity_flagged() {
    let mut record = make_record_with_narration(
        "Suddenly, Mordecai appears from the shadows and blocks the path.",
    );
    record.snapshot_after.characters = vec![make_character("Kael", vec![])];

    let results = check_entity_references(&record);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert!(
        !warnings.is_empty(),
        "Unknown entity 'Mordecai' should produce a warning"
    );
    assert!(
        results.iter().any(|r| match r {
            ValidationResult::Warning(msg) => msg.contains("Mordecai"),
            _ => false,
        }),
        "Warning should mention 'Mordecai'; got: {:?}",
        results
    );
}

#[test]
fn compound_name_substring_match_no_warning() {
    let mut record = make_record_with_narration(
        "The crowd parts as Old Grimjaw slams his fist on the table.",
    );
    record.snapshot_after.npcs = vec![make_npc("Grimjaw")];

    let results = check_entity_references(&record);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(msg) if msg.contains("Grimjaw")))
        .collect();
    assert!(
        warnings.is_empty(),
        "'Old Grimjaw' should match NPC 'Grimjaw' via substring; got: {:?}",
        results
    );
}

#[test]
fn known_item_not_flagged() {
    let mut record = make_record_with_narration(
        "Light glints off the Flaming Sword as it arcs through the air.",
    );
    record.snapshot_after.characters = vec![make_character("Kael", vec!["Flaming Sword"])];

    let results = check_entity_references(&record);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(msg) if msg.contains("Flaming")))
        .collect();
    assert!(
        warnings.is_empty(),
        "Known item 'Flaming Sword' should not be flagged; got: {:?}",
        results
    );
}

#[test]
fn known_npc_not_flagged() {
    let mut record = make_record_with_narration(
        "The merchant looks up as Grimjaw enters the tavern.",
    );
    record.snapshot_after.npcs = vec![make_npc("Grimjaw")];

    let results = check_entity_references(&record);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(msg) if msg.contains("Grimjaw")))
        .collect();
    assert!(
        warnings.is_empty(),
        "Known NPC 'Grimjaw' should not be flagged; got: {:?}",
        results
    );
}

#[test]
fn sentence_initial_caps_not_flagged() {
    let mut record = make_record_with_narration(
        "Darkness falls. Shadows creep across the floor. Thunder rolls.",
    );
    record.snapshot_after.characters = vec![make_character("Kael", vec![])];

    let results = check_entity_references(&record);

    assert!(
        results.is_empty(),
        "Sentence-initial capitalized words should not produce warnings; got: {:?}",
        results
    );
}

#[test]
fn multiple_unknown_entities_each_flagged() {
    let mut record = make_record_with_narration(
        "The battle rages as Mordecai and Zephira clash in the courtyard.",
    );
    record.snapshot_after.characters = vec![make_character("Kael", vec![])];

    let results = check_entity_references(&record);

    let warnings: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Warning(_)))
        .collect();
    assert!(
        warnings.len() >= 2,
        "Should flag both 'Mordecai' and 'Zephira'; got {} warnings: {:?}",
        warnings.len(),
        results
    );
}

#[test]
fn empty_narration_produces_no_warnings() {
    let record = make_record_with_narration("");

    let results = check_entity_references(&record);

    assert!(
        results.is_empty(),
        "Empty narration should produce no warnings; got: {:?}",
        results
    );
}

// ===========================================================================
// Tracing emission tests
// ===========================================================================

#[test]
fn unresolved_reference_emits_tracing_warn() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let mut record = make_record_with_narration(
        "Without warning, Mordecai leaps from the rafters.",
    );
    record.snapshot_after.characters = vec![make_character("Kael", vec![])];

    tracing::subscriber::with_default(subscriber, || {
        check_entity_references(&record);
    });

    let events = captured.lock().unwrap();
    let warnings = entity_ref_warnings(&events);

    assert!(
        !warnings.is_empty(),
        "Should emit tracing::warn! for unresolved entity 'Mordecai'"
    );
    assert!(
        warnings.iter().any(|e| {
            e.field_value("unresolved")
                .map_or(false, |v| v.contains("Mordecai"))
        }),
        "Warning event should have unresolved field containing 'Mordecai'; events: {:?}",
        *events
    );
}

#[test]
fn known_entity_does_not_emit_tracing_warn() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let mut record = make_record_with_narration(
        "The crowd watches as Kael enters the arena.",
    );
    record.snapshot_after.characters = vec![make_character("Kael", vec![])];

    tracing::subscriber::with_default(subscriber, || {
        check_entity_references(&record);
    });

    let events = captured.lock().unwrap();
    let warnings = entity_ref_warnings(&events);

    assert!(
        warnings.is_empty(),
        "Known entity 'Kael' should not emit any tracing warnings; got {:?}",
        *events
    );
}

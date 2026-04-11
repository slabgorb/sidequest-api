//! Story 31-3: Wire equipment_generation random_table in CharacterBuilder
//!
//! RED phase — tests exercise equipment table loading, composition, and wiring
//! into the CharacterBuilder state machine. Will fail until Dev implements:
//!   - EquipmentTables struct in sidequest-genre (with custom Deserialize like BackstoryTables)
//!   - Re-export at sidequest_genre crate root
//!   - GenrePack::equipment_tables optional field + loader.rs wiring (equipment_tables.yaml)
//!   - CharacterBuilder::with_equipment_tables(...) fluent setter
//!   - build() composition: when scene has mechanical_effects.equipment_generation == "random_table"
//!     AND self.equipment_tables is set, roll one item per slot and append to inventory
//!   - OTEL span: tracing::info_span!("chargen.equipment_composed", method = "tables" | "fallback")
//!   - dispatch/connect.rs wiring: call .with_equipment_tables(pack.equipment_tables.clone())
//!
//! ACs tested:
//!   1. C&C scenes with `equipment_generation: random_table` produce inventory from tables (not hardcoded)
//!   2. Builders without equipment_tables fall through to existing item_hints behavior unchanged
//!   3. equipment_tables.yaml is optional per genre (loader tolerates absence)
//!   4. Every generated Item has a non-blank id and a non-blank name
//!   5. Equipment selection varies between builds (randomness)
//!   6. When rolls_per_slot is unset, each slot yields exactly one item
//!   7. Unreplaced placeholder leakage is impossible (EquipmentTables picks item ids, not template strings)
//!   8. Wiring: EquipmentTables is re-exported from sidequest_genre at crate root
//!   9. Serialization round-trip: EquipmentTables deserializes from the equipment_tables.yaml shape
//!
//! Rust lang-review checks enforced:
//!   - Check 4 (Tracing): span emission is tested via tracing_subscriber capture
//!   - Check 6 (Test quality): all assertions are meaningful, no let _ = result
//!   - Check 9 (Public fields): EquipmentTables fields exposed for read-only serialization test
//!   - Check 13 (Deserialize consistency): YAML deserialization produces same shape as direct construction

use std::collections::HashMap;

use sidequest_genre::{
    BackstoryTables, CharCreationChoice, CharCreationScene, EquipmentTables, MechanicalEffects,
    RulesConfig,
};

use sidequest_game::builder::{BuilderError, CharacterBuilder};

// ============================================================================
// Test fixtures — scenes, rules, tables
// ============================================================================

/// Caverns & Claudes-style scenes — includes `the_kit` scene with
/// `equipment_generation: random_table` directive to trigger the new wiring.
fn caverns_scenes_with_kit() -> Vec<CharCreationScene> {
    vec![
        CharCreationScene {
            id: "the_roll".to_string(),
            title: "3d6. In Order.".to_string(),
            narration: "The man with no fingers pushes six bone dice.".to_string(),
            choices: vec![],
            allows_freeform: Some(false),
            hook_prompt: None,
            loading_text: None,
            mechanical_effects: Some(MechanicalEffects {
                stat_generation: Some("roll_3d6_strict".to_string()),
                ..MechanicalEffects::default()
            }),
        },
        CharCreationScene {
            id: "pronouns".to_string(),
            title: "Who Are You?".to_string(),
            narration: "For the tally.".to_string(),
            choices: vec![CharCreationChoice {
                label: "he/him".to_string(),
                description: "He.".to_string(),
                mechanical_effects: MechanicalEffects {
                    pronoun_hint: Some("he/him".to_string()),
                    ..MechanicalEffects::default()
                },
            }],
            allows_freeform: Some(false),
            hook_prompt: None,
            loading_text: None,
            mechanical_effects: None,
        },
        CharCreationScene {
            id: "the_kit".to_string(),
            title: "What You Have".to_string(),
            narration: "He drops a canvas sack on the table.".to_string(),
            choices: vec![],
            allows_freeform: Some(false),
            hook_prompt: None,
            loading_text: None,
            mechanical_effects: Some(MechanicalEffects {
                equipment_generation: Some("random_table".to_string()),
                ..MechanicalEffects::default()
            }),
        },
    ]
}

/// Same scene flow but without the `the_kit` directive — used to verify that
/// non-directive scenes do not trigger equipment rolls.
fn caverns_scenes_without_kit() -> Vec<CharCreationScene> {
    let mut scenes = caverns_scenes_with_kit();
    // Drop the_kit scene entirely
    scenes.pop();
    scenes
}

fn rules_3d6() -> RulesConfig {
    RulesConfig {
        tone: "gritty".to_string(),
        lethality: "high".to_string(),
        magic_level: "none".to_string(),
        stat_generation: "roll_3d6_strict".to_string(),
        point_buy_budget: 0,
        ability_score_names: vec![
            "STR".to_string(),
            "DEX".to_string(),
            "CON".to_string(),
            "INT".to_string(),
            "WIS".to_string(),
            "CHA".to_string(),
        ],
        allowed_classes: vec!["Delver".to_string()],
        allowed_races: vec!["Human".to_string()],
        class_hp_bases: HashMap::from([("Delver".to_string(), 8)]),
        default_class: Some("Delver".to_string()),
        default_race: Some("Human".to_string()),
        default_hp: Some(8),
        default_ac: Some(10),
        default_location: Some("The mouth of the dungeon".to_string()),
        default_time_of_day: Some("dawn".to_string()),
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

/// Three-slot equipment table: weapon / armor / utility.
/// Each slot has multiple candidates so we can verify random selection variance.
fn test_equipment_tables() -> EquipmentTables {
    EquipmentTables {
        tables: HashMap::from([
            (
                "weapon".to_string(),
                vec![
                    "dagger_iron".to_string(),
                    "shortsword_iron".to_string(),
                    "quarterstaff".to_string(),
                    "club_heavy".to_string(),
                ],
            ),
            (
                "armor".to_string(),
                vec![
                    "leather_armor".to_string(),
                    "studded_leather".to_string(),
                    "padded_cloth".to_string(),
                ],
            ),
            (
                "utility".to_string(),
                vec![
                    "rope_hemp".to_string(),
                    "torch".to_string(),
                    "iron_spikes".to_string(),
                    "chalk".to_string(),
                ],
            ),
        ]),
        rolls_per_slot: HashMap::new(),
    }
}

/// All candidate item ids flattened — used to verify selected items came from tables.
fn all_table_item_ids() -> Vec<String> {
    let tables = test_equipment_tables();
    tables
        .tables
        .values()
        .flat_map(|v| v.iter().cloned())
        .collect()
}

/// Drive a CharacterBuilder through the three C&C scenes and build.
fn build_caverns_character_with_tables() -> Result<sidequest_game::Character, BuilderError> {
    let scenes = caverns_scenes_with_kit();
    let rules = rules_3d6();
    let mut builder =
        CharacterBuilder::new(scenes, &rules, None).with_equipment_tables(test_equipment_tables());
    builder.apply_freeform("")?; // the_roll
    builder.apply_choice(0)?; // pronouns
    builder.apply_freeform("")?; // the_kit (display-only, continue)
    builder.build("Grist")
}

/// Build a C&C character WITHOUT equipment_tables wired in.
/// Existing fallback behavior (item_hints from choices) must still work.
fn build_caverns_character_without_tables() -> Result<sidequest_game::Character, BuilderError> {
    let scenes = caverns_scenes_without_kit();
    let rules = rules_3d6();
    let mut builder = CharacterBuilder::new(scenes, &rules, None);
    builder.apply_freeform("")?; // the_roll
    builder.apply_choice(0)?; // pronouns
    builder.build("Grist")
}

// ============================================================================
// AC-1: C&C scenes with `equipment_generation: random_table` populate inventory from tables
// ============================================================================

#[test]
fn caverns_character_gets_equipment_from_tables() {
    let character = build_caverns_character_with_tables().expect("build should succeed");
    let item_count = character.core.inventory.items.len();

    // With three tables and no rolls_per_slot overrides, expect exactly three items.
    assert_eq!(
        item_count,
        3,
        "Expected 3 items (one per table slot), got {}. Inventory: {:?}",
        item_count,
        character
            .core
            .inventory
            .items
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn caverns_character_equipment_items_come_from_tables() {
    let character = build_caverns_character_with_tables().expect("build should succeed");
    let candidates = all_table_item_ids();

    for item in &character.core.inventory.items {
        let id = item.id.as_str();
        assert!(
            candidates.iter().any(|c| c == id),
            "Generated item id '{}' is not in any configured table slot. Candidates: {:?}",
            id,
            candidates
        );
    }
}

#[test]
fn caverns_character_covers_all_table_slots() {
    // A single roll produces one item per slot — verify at least one item
    // was picked from each of the three slot candidate pools over 20 builds
    // (accounts for random selection within each slot).
    let tables = test_equipment_tables();
    let mut seen_per_slot: HashMap<&str, bool> = HashMap::new();
    seen_per_slot.insert("weapon", false);
    seen_per_slot.insert("armor", false);
    seen_per_slot.insert("utility", false);

    for _ in 0..20 {
        let character = build_caverns_character_with_tables().expect("build should succeed");
        for item in &character.core.inventory.items {
            let id = item.id.as_str();
            for (slot, candidates) in &tables.tables {
                if candidates.iter().any(|c| c == id) {
                    seen_per_slot.insert(slot.as_str(), true);
                }
            }
        }
    }

    for (slot, seen) in &seen_per_slot {
        assert!(
            *seen,
            "After 20 builds, never saw an item from slot '{}' — selection may be skipping slots",
            slot
        );
    }
}

// ============================================================================
// AC-2: Builders without equipment_tables retain existing item_hints fallback
// ============================================================================

#[test]
fn builder_without_equipment_tables_still_builds() {
    let character = build_caverns_character_without_tables().expect("build should succeed");
    // No panic, no crash. Inventory may be empty (no item_hints in these scenes) but build succeeds.
    assert_eq!(
        character.core.name.as_str(),
        "Grist",
        "Character should build successfully without equipment tables"
    );
}

#[test]
fn builder_without_equipment_tables_has_no_table_sourced_items() {
    let character = build_caverns_character_without_tables().expect("build should succeed");
    let candidates = all_table_item_ids();

    // None of the hardcoded table candidates should appear — there's no equipment_tables wired
    // AND no item_hints in the caverns_scenes_without_kit scenes.
    for item in &character.core.inventory.items {
        let id = item.id.as_str();
        assert!(
            !candidates.iter().any(|c| c == id),
            "Unexpected item '{}' in inventory — builder without tables should not produce table items",
            id
        );
    }
}

// ============================================================================
// AC-3: equipment_tables is optional per genre (no panic when None)
// ============================================================================

#[test]
fn builder_accepts_none_equipment_tables_via_default() {
    // Default builder has no equipment_tables. The setter is fluent and optional.
    let scenes = caverns_scenes_with_kit();
    let rules = rules_3d6();
    let mut builder = CharacterBuilder::new(scenes, &rules, None);
    builder.apply_freeform("").unwrap(); // the_roll
    builder.apply_choice(0).unwrap(); // pronouns
    builder.apply_freeform("").unwrap(); // the_kit (directive present, but no tables wired)

    // Building should NOT crash when a scene has equipment_generation: random_table
    // but the builder has no equipment_tables. It should gracefully skip the roll.
    let result = builder.build("Nobody");
    assert!(
        result.is_ok(),
        "Build must succeed even when scene has equipment_generation directive without tables: {:?}",
        result.err()
    );
}

// ============================================================================
// AC-4: Every generated Item has valid (non-blank) id and name
// ============================================================================

#[test]
fn generated_items_have_nonblank_id_and_name() {
    let character = build_caverns_character_with_tables().expect("build should succeed");

    for item in &character.core.inventory.items {
        assert!(
            !item.id.as_str().trim().is_empty(),
            "Generated item has blank id: {:?}",
            item
        );
        assert!(
            !item.name.as_str().trim().is_empty(),
            "Generated item has blank name: {:?}",
            item
        );
    }
}

#[test]
fn generated_items_are_marked_carried() {
    use sidequest_game::inventory::ItemState;
    let character = build_caverns_character_with_tables().expect("build should succeed");
    assert!(
        !character.core.inventory.items.is_empty(),
        "Inventory must be non-empty for this assertion to mean anything"
    );
    for item in &character.core.inventory.items {
        assert_eq!(
            item.state,
            ItemState::Carried,
            "Generated starting equipment must be in Carried state, not {:?}",
            item.state
        );
    }
}

// ============================================================================
// AC-5: Equipment selection varies between builds (randomness)
// ============================================================================

#[test]
fn equipment_varies_between_builds() {
    let mut seen_item_sets: Vec<Vec<String>> = Vec::new();
    for _ in 0..20 {
        let character = build_caverns_character_with_tables().expect("build should succeed");
        let mut ids: Vec<String> = character
            .core
            .inventory
            .items
            .iter()
            .map(|i| i.id.as_str().to_string())
            .collect();
        ids.sort();
        seen_item_sets.push(ids);
    }

    let unique: std::collections::HashSet<&Vec<String>> = seen_item_sets.iter().collect();
    assert!(
        unique.len() > 1,
        "20 builds should produce more than 1 distinct equipment set. Got {} unique.",
        unique.len()
    );
}

// ============================================================================
// AC-6: Without rolls_per_slot, each slot yields exactly one item
// ============================================================================

#[test]
fn default_rolls_per_slot_produces_one_item_per_slot() {
    let character = build_caverns_character_with_tables().expect("build should succeed");
    let tables = test_equipment_tables();
    assert_eq!(
        character.core.inventory.items.len(),
        tables.tables.len(),
        "Without rolls_per_slot, items.len() must equal number of table slots"
    );
}

#[test]
fn rolls_per_slot_multiplies_item_count() {
    // With rolls_per_slot override: utility → 3 rolls, weapon → 1, armor → 1.
    // Expected total: 1 + 1 + 3 = 5 items.
    let scenes = caverns_scenes_with_kit();
    let rules = rules_3d6();
    let tables = EquipmentTables {
        tables: HashMap::from([
            (
                "weapon".to_string(),
                vec!["dagger_iron".to_string(), "shortsword_iron".to_string()],
            ),
            ("armor".to_string(), vec!["leather_armor".to_string()]),
            (
                "utility".to_string(),
                vec![
                    "torch".to_string(),
                    "rope_hemp".to_string(),
                    "iron_spikes".to_string(),
                    "chalk".to_string(),
                ],
            ),
        ]),
        rolls_per_slot: HashMap::from([("utility".to_string(), 3)]),
    };

    let mut builder = CharacterBuilder::new(scenes, &rules, None).with_equipment_tables(tables);
    builder.apply_freeform("").unwrap();
    builder.apply_choice(0).unwrap();
    builder.apply_freeform("").unwrap();
    let character = builder.build("Grist").expect("build should succeed");

    assert_eq!(
        character.core.inventory.items.len(),
        5,
        "Expected 5 items total (1 weapon + 1 armor + 3 utility), got {}",
        character.core.inventory.items.len()
    );
}

// ============================================================================
// AC-7: Empty table slots do not crash and produce no item from that slot
// ============================================================================

#[test]
fn empty_slot_produces_no_item() {
    let scenes = caverns_scenes_with_kit();
    let rules = rules_3d6();
    let tables = EquipmentTables {
        tables: HashMap::from([
            ("weapon".to_string(), vec!["dagger_iron".to_string()]),
            // Intentionally empty slot — must not crash, must not produce a blank-id item
            ("armor".to_string(), vec![]),
            ("utility".to_string(), vec!["torch".to_string()]),
        ]),
        rolls_per_slot: HashMap::new(),
    };

    let mut builder = CharacterBuilder::new(scenes, &rules, None).with_equipment_tables(tables);
    builder.apply_freeform("").unwrap();
    builder.apply_choice(0).unwrap();
    builder.apply_freeform("").unwrap();
    let character = builder
        .build("Grist")
        .expect("build should succeed with empty slot");

    // Must have exactly 2 items (weapon + utility, no armor from empty slot).
    assert_eq!(
        character.core.inventory.items.len(),
        2,
        "Empty slot should skip item generation, not crash or insert blank. Items: {:?}",
        character
            .core
            .inventory
            .items
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>()
    );
    // No blank ids
    for item in &character.core.inventory.items {
        assert!(
            !item.id.as_str().is_empty(),
            "Empty-slot handling must not produce blank-id items"
        );
    }
}

// ============================================================================
// AC-8: Scene without equipment_generation directive does not consume equipment tables
// ============================================================================

#[test]
fn scene_without_directive_does_not_roll_equipment() {
    // Use scenes WITHOUT the_kit (no equipment_generation directive) but attach
    // equipment_tables anyway. Tables should remain dormant.
    let scenes = caverns_scenes_without_kit();
    let rules = rules_3d6();
    let mut builder =
        CharacterBuilder::new(scenes, &rules, None).with_equipment_tables(test_equipment_tables());
    builder.apply_freeform("").unwrap();
    builder.apply_choice(0).unwrap();
    let character = builder.build("Grist").expect("build should succeed");

    let candidates = all_table_item_ids();
    for item in &character.core.inventory.items {
        let id = item.id.as_str();
        assert!(
            !candidates.iter().any(|c| c == id),
            "Item '{}' from tables appeared without a scene directive — directive gate broken",
            id
        );
    }
}

// ============================================================================
// AC-9: Works alongside backstory_tables (no interference between the two systems)
// ============================================================================

#[test]
fn equipment_and_backstory_tables_coexist() {
    let scenes = caverns_scenes_with_kit();
    let rules = rules_3d6();
    let backstory = BackstoryTables {
        template: "Former {trade}. {feature}.".to_string(),
        tables: HashMap::from([
            (
                "trade".to_string(),
                vec!["ratcatcher".to_string(), "gravedigger".to_string()],
            ),
            (
                "feature".to_string(),
                vec!["Missing three fingers".to_string()],
            ),
        ]),
    };
    let mut builder = CharacterBuilder::new(scenes, &rules, Some(backstory))
        .with_equipment_tables(test_equipment_tables());
    builder.apply_freeform("").unwrap();
    builder.apply_choice(0).unwrap();
    builder.apply_freeform("").unwrap();
    let character = builder.build("Grist").expect("build should succeed");

    // Backstory comes from backstory_tables
    assert!(
        character.backstory.as_str().starts_with("Former"),
        "Backstory should use backstory_tables template. Got: {}",
        character.backstory.as_str()
    );
    // Equipment comes from equipment_tables
    assert_eq!(
        character.core.inventory.items.len(),
        3,
        "Equipment should come from equipment_tables (3 slots → 3 items)"
    );
}

// ============================================================================
// Rule: EquipmentTables deserializes from YAML matching the equipment_tables.yaml shape
// ============================================================================

#[test]
fn equipment_tables_deserializes_from_yaml() {
    let yaml = r#"
tables:
  weapon:
    - dagger_iron
    - shortsword_iron
  armor:
    - leather_armor
  utility:
    - torch
    - rope_hemp
rolls_per_slot:
  utility: 2
"#;
    let parsed: EquipmentTables =
        serde_yaml::from_str(yaml).expect("EquipmentTables must deserialize from valid YAML");

    assert_eq!(
        parsed.tables.len(),
        3,
        "Expected 3 table slots from YAML, got {}",
        parsed.tables.len()
    );
    assert_eq!(
        parsed.tables.get("weapon").map(|v| v.len()),
        Some(2),
        "weapon slot should have 2 entries"
    );
    assert_eq!(
        parsed.rolls_per_slot.get("utility").copied(),
        Some(2),
        "rolls_per_slot.utility should be 2"
    );
}

#[test]
fn equipment_tables_rejects_missing_tables_field() {
    // `tables:` is required. A document without it must fail to deserialize.
    let yaml = r#"
rolls_per_slot:
  utility: 2
"#;
    let parsed: Result<EquipmentTables, _> = serde_yaml::from_str(yaml);
    assert!(
        parsed.is_err(),
        "Missing 'tables' field must be a hard deserialization error, not silently defaulted"
    );
}

// ============================================================================
// Wiring: EquipmentTables is exported at sidequest_genre crate root
// ============================================================================

#[test]
fn equipment_tables_is_reexported_at_crate_root() {
    // If this compiles and runs, the re-export exists. The act of constructing
    // a value proves the type is public and the path is stable.
    let tables = sidequest_genre::EquipmentTables {
        tables: HashMap::from([("test".to_string(), vec!["item_a".to_string()])]),
        rolls_per_slot: HashMap::new(),
    };
    assert_eq!(tables.tables.len(), 1);
}

// ============================================================================
// Wiring: CharacterBuilder::with_equipment_tables is a fluent setter
// ============================================================================

#[test]
fn with_equipment_tables_returns_self_for_chaining() {
    let scenes = caverns_scenes_without_kit();
    let rules = rules_3d6();
    // Chaining the setter from `new()` must compile and produce a working builder.
    let builder =
        CharacterBuilder::new(scenes, &rules, None).with_equipment_tables(test_equipment_tables());
    // A newly-constructed builder is in InProgress phase.
    assert!(
        !builder.is_confirmation(),
        "Freshly-constructed builder should not be in Confirmation phase"
    );
}

// ============================================================================
// Wiring integration: server dispatch calls .with_equipment_tables(...)
// This is the "every test suite needs a wiring test" enforcement. We can't
// easily boot the server here, but we CAN verify the production connect.rs
// file references the setter (string-level wiring test).
// ============================================================================

#[test]
fn dispatch_connect_wires_equipment_tables_into_builder() {
    // Find the dispatch/connect.rs file relative to the workspace root.
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // From crates/sidequest-game → ../sidequest-server/src/dispatch/connect.rs
    path.pop(); // sidequest-game
    path.push("sidequest-server");
    path.push("src");
    path.push("dispatch");
    path.push("connect.rs");

    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Could not read dispatch/connect.rs at {:?}: {}", path, e));

    assert!(
        source.contains("with_equipment_tables"),
        "dispatch/connect.rs must call .with_equipment_tables(pack.equipment_tables.clone()) \
         after CharacterBuilder::try_new — this is the production wiring gate. Not found in {:?}",
        path
    );
    assert!(
        source.contains("equipment_tables"),
        "dispatch/connect.rs must reference pack.equipment_tables for the wiring to be live"
    );
}

// ============================================================================
// Wiring: GenrePack exposes equipment_tables field (compile-time check)
// ============================================================================

#[test]
fn genre_pack_has_equipment_tables_field() {
    // This is a compile-time check expressed as a runtime test — if the field
    // doesn't exist, this test file won't compile.
    fn _assert_field(pack: &sidequest_genre::GenrePack) -> &Option<EquipmentTables> {
        &pack.equipment_tables
    }
    // Running the test just proves the function resolved.
    // (Function pointer comparison is stable only for fn items.)
    let _f: fn(&sidequest_genre::GenrePack) -> &Option<EquipmentTables> = _assert_field;
}

// ============================================================================
// REWORK (after Reviewer rejection 2026-04-10): Watcher channel emission
//
// The original implementation used `tracing::info!(target: "chargen.equipment_composed")`
// which does NOT reach the GM panel watcher broadcast channel — there is no
// production tracing::Layer bridging tracing events to the sidequest-telemetry
// channel. Per CLAUDE.md OTEL Observability Principle, chargen telemetry MUST
// reach the GM panel via `watcher!` / `WatcherEventBuilder`.
//
// These tests verify the correct emission path. They will fail until Dev
// swaps `tracing::info!` for `watcher!("chargen", StateTransition, ...)` and
// adds missing emissions at the blank-id skip and the "none" branch.
// ============================================================================

use sidequest_telemetry::{init_global_channel, subscribe_global, Severity, WatcherEvent};

/// Serializes access to the global telemetry channel so concurrent tests
/// don't drain each other's events.
static TELEMETRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Acquire the telemetry lock, initialize the channel (idempotent), and return
/// a drained receiver ready for the test's events. Recovers from a poisoned
/// mutex so that the first test-assertion panic doesn't cascade into every
/// subsequent test.
fn fresh_subscriber() -> (
    std::sync::MutexGuard<'static, ()>,
    tokio::sync::broadcast::Receiver<WatcherEvent>,
) {
    let guard = TELEMETRY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = init_global_channel();
    let mut rx = subscribe_global().expect("channel must be initialized");
    while rx.try_recv().is_ok() {}
    (guard, rx)
}

/// Drain all currently-available events from the receiver (non-blocking).
fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<WatcherEvent>) -> Vec<WatcherEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

/// Find all events on the `chargen` component whose `action` field matches.
fn find_chargen_events(events: &[WatcherEvent], action: &str) -> Vec<WatcherEvent> {
    events
        .iter()
        .filter(|e| {
            e.component == "chargen"
                && e.fields
                    .get("action")
                    .and_then(serde_json::Value::as_str)
                    == Some(action)
        })
        .cloned()
        .collect()
}

// ----------------------------------------------------------------------------
// AC (rework-1): Successful equipment roll emits a chargen watcher event
// ----------------------------------------------------------------------------

#[test]
fn watcher_channel_receives_chargen_equipment_composed_event_on_successful_roll() {
    let (_guard, mut rx) = fresh_subscriber();

    let _character = build_caverns_character_with_tables().expect("build should succeed");

    let events = drain_events(&mut rx);
    let composed = find_chargen_events(&events, "equipment_composed");

    assert!(
        !composed.is_empty(),
        "Building a character with equipment_tables must emit a `chargen` component WatcherEvent \
         with action=equipment_composed. Got {} chargen events total: {:?}",
        events.iter().filter(|e| e.component == "chargen").count(),
        events
            .iter()
            .filter(|e| e.component == "chargen")
            .map(|e| {
                e.fields
                    .get("action")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("<no action>")
                    .to_string()
            })
            .collect::<Vec<_>>()
    );

    let event = &composed[0];
    assert_eq!(
        event
            .fields
            .get("method")
            .and_then(serde_json::Value::as_str),
        Some("tables"),
        "Event method field must be 'tables' for the successful-roll path. Got: {:?}",
        event.fields.get("method")
    );

    let items_added = event
        .fields
        .get("items_added")
        .and_then(serde_json::Value::as_i64);
    assert!(
        items_added.is_some() && items_added.unwrap() > 0,
        "Event must include items_added > 0 for a successful roll. Got: {:?}",
        event.fields.get("items_added")
    );
}

// ----------------------------------------------------------------------------
// AC (rework-2): Scene directive with no tables wired emits a Warn event
// ----------------------------------------------------------------------------

#[test]
fn watcher_channel_receives_warn_when_equipment_directive_has_no_tables() {
    let (_guard, mut rx) = fresh_subscriber();

    // Build a character with the `the_kit` scene directive but NO equipment_tables
    // wired into the builder. This is the misconfiguration path — must not be silent.
    let scenes = caverns_scenes_with_kit();
    let rules = rules_3d6();
    let mut builder = CharacterBuilder::new(scenes, &rules, None);
    builder.apply_freeform("").unwrap(); // the_roll
    builder.apply_choice(0).unwrap(); // pronouns
    builder.apply_freeform("").unwrap(); // the_kit (directive present, no tables)
    let _character = builder.build("Grist").expect("build should succeed");

    let events = drain_events(&mut rx);

    // The missing-tables path should surface a specific action tag.
    let missing = find_chargen_events(&events, "equipment_tables_missing");
    assert!(
        !missing.is_empty(),
        "A scene with `equipment_generation: random_table` directive but no wired tables must emit \
         a chargen WatcherEvent with action=equipment_tables_missing. This is a misconfiguration, \
         not graceful degradation, per CLAUDE.md 'No Silent Fallbacks'. Got events: {:?}",
        events
            .iter()
            .filter(|e| e.component == "chargen")
            .map(|e| e.fields.get("action").cloned())
            .collect::<Vec<_>>()
    );

    let event = &missing[0];
    assert!(
        matches!(event.severity, Severity::Warn | Severity::Error),
        "equipment_tables_missing event must be Warn or Error severity (misconfiguration). \
         Got: {:?}",
        event.severity
    );
}

// ----------------------------------------------------------------------------
// AC (rework-3): Blank item id in tables emits a skip Warn event
// ----------------------------------------------------------------------------

#[test]
fn watcher_channel_receives_warn_on_blank_item_id_skip() {
    let (_guard, mut rx) = fresh_subscriber();

    // Equipment tables with an intentional blank id to trigger the skip path.
    let scenes = caverns_scenes_with_kit();
    let rules = rules_3d6();
    let tables = EquipmentTables {
        tables: HashMap::from([(
            "weapon".to_string(),
            vec![
                "".to_string(), // blank — should trigger the skip + warn
                "dagger_iron".to_string(),
            ],
        )]),
        rolls_per_slot: HashMap::from([("weapon".to_string(), 20)]), // force multiple rolls to hit the blank
    };
    let mut builder = CharacterBuilder::new(scenes, &rules, None).with_equipment_tables(tables);
    builder.apply_freeform("").unwrap();
    builder.apply_choice(0).unwrap();
    builder.apply_freeform("").unwrap();
    let _character = builder.build("Grist").expect("build should succeed");

    let events = drain_events(&mut rx);
    let skipped = find_chargen_events(&events, "blank_item_id_skipped");

    assert!(
        !skipped.is_empty(),
        "A blank item_id in equipment_tables must emit a chargen WatcherEvent with \
         action=blank_item_id_skipped so content bugs surface to the GM panel. \
         The previous `tracing::warn!` does NOT reach the watcher channel. Got events: {:?}",
        events
            .iter()
            .filter(|e| e.component == "chargen")
            .map(|e| e.fields.get("action").cloned())
            .collect::<Vec<_>>()
    );

    let event = &skipped[0];
    assert_eq!(
        event.fields.get("slot").and_then(serde_json::Value::as_str),
        Some("weapon"),
        "blank_item_id_skipped event must include the slot name. Got: {:?}",
        event.fields.get("slot")
    );
    assert!(
        matches!(event.severity, Severity::Warn | Severity::Error),
        "blank_item_id_skipped event must be Warn or Error severity. Got: {:?}",
        event.severity
    );
}

// ----------------------------------------------------------------------------
// AC (rework-4): Successful roll events carry component="chargen" not "tracing"
//
// Sanity check that the emission uses the WatcherEventBuilder/watcher! path
// with a correct component name — catches the case where Dev uses
// `watcher!("tracing", ...)` or some other component string by accident.
// ----------------------------------------------------------------------------

#[test]
fn equipment_watcher_events_use_chargen_component() {
    let (_guard, mut rx) = fresh_subscriber();

    let _character = build_caverns_character_with_tables().expect("build should succeed");

    let events = drain_events(&mut rx);
    let chargen_events: Vec<&WatcherEvent> =
        events.iter().filter(|e| e.component == "chargen").collect();

    assert!(
        !chargen_events.is_empty(),
        "Equipment composition must emit at least one WatcherEvent with component='chargen'. \
         This is a CLAUDE.md OTEL Observability requirement — the GM panel filters by component."
    );
}

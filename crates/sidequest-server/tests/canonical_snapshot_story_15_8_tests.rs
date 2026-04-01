//! Story 15-8: Canonical GameSnapshot in dispatch — eliminate load-before-save round-trip
//!
//! RED phase — these tests verify that persist_game_state() uses the canonical
//! GameSnapshot from DispatchContext instead of loading from SQLite on every turn.
//!
//! Structural tests read dispatch/mod.rs source to verify:
//!   1. persist_game_state() does NOT call persistence().load() on the save path
//!   2. DispatchContext carries a `snapshot` field (GameSnapshot)
//!   3. OTEL watcher event for persistence.save_latency_ms is emitted
//!   4. Save path uses ctx.snapshot directly, not a freshly loaded snapshot
//!
//! Behavioral tests verify:
//!   5. GameSnapshot round-trip through persistence preserves all dispatch-relevant fields
//!   6. Save-only path (no prior load) works correctly
//!   7. Session restore (load path) still works for reconnect
//!
//! ACs covered:
//!   AC-1: persist_game_state() does NOT call persistence().load() on the save path
//!   AC-2: OTEL span persistence.save_latency_ms emits on every turn
//!   AC-3: Multi-turn session completes without errors (via snapshot patching)
//!   AC-4: Session restore on reconnect still works (loads from SQLite)
//!   AC-5: Unit tests pass
//!   AC-6: No silent fallbacks, proper error handling

use std::collections::HashMap;

use sidequest_game::character::Character;
use sidequest_game::combat::CombatState;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::{Inventory, Item};
use sidequest_game::narrative::NarrativeEntry;
use sidequest_game::persistence::{SessionStore, SqliteStore};
use sidequest_game::state::GameSnapshot;
use sidequest_game::turn::TurnManager;
use sidequest_protocol::NonBlankString;

// ============================================================================
// Test fixtures
// ============================================================================

/// Build a test item with minimal required fields.
fn test_item(id: &str, name: &str) -> Item {
    Item {
        id: NonBlankString::new(id).unwrap(),
        name: NonBlankString::new(name).unwrap(),
        description: NonBlankString::new("A test item").unwrap(),
        category: NonBlankString::new("tool").unwrap(),
        value: 1,
        weight: 1.0,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.0,
        tags: vec![],
        equipped: false,
        quantity: 1,
    }
}

/// Build a GameSnapshot with all dispatch-relevant fields populated.
/// This represents the canonical state that DispatchContext.snapshot should carry.
fn dispatch_snapshot() -> GameSnapshot {
    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        characters: vec![Character {
            core: CreatureCore {
                name: NonBlankString::new("Thorn").unwrap(),
                description: NonBlankString::new("A scarred warrior").unwrap(),
                personality: NonBlankString::new("Gruff").unwrap(),
                level: 3,
                hp: 18,
                max_hp: 25,
                ac: 12,
                xp: 450,
                inventory: {
                    let mut inv = Inventory::default();
                    inv.add(test_item("rusty_blade", "Rusty Blade"), 20).unwrap();
                    inv.add(test_item("healing_salve", "Healing Salve"), 20).unwrap();
                    inv
                },
                statuses: vec![],
            },
            backstory: NonBlankString::new("Born in the wastes").unwrap(),
            narrative_state: "Exploring the ruins".to_string(),
            hooks: vec!["Find the lost caravan".to_string()],
            char_class: NonBlankString::new("Fighter").unwrap(),
            race: NonBlankString::new("Human").unwrap(),
            pronouns: "he/him".to_string(),
            stats: HashMap::from([
                ("STR".to_string(), 14),
                ("DEX".to_string(), 12),
                ("CON".to_string(), 13),
                ("INT".to_string(), 10),
                ("WIS".to_string(), 8),
                ("CHA".to_string(), 15),
            ]),
            abilities: vec![],
            known_facts: vec![],
            affinities: vec![],
            is_friendly: true,
        }],
        npcs: vec![],
        location: "Rusted Bazaar".to_string(),
        time_of_day: "dusk".to_string(),
        quest_log: HashMap::from([
            ("Find the Caravan".to_string(), "Track the lost supply caravan through the wastes".to_string()),
        ]),
        notes: vec![],
        narrative_log: vec![
            NarrativeEntry {
                timestamp: 1000,
                round: 1,
                author: "narrator".to_string(),
                content: "The wastes stretch endlessly before you.".to_string(),
                tags: vec![],
                encounter_tags: vec![],
                speaker: None,
                entry_type: None,
            },
        ],
        combat: CombatState::new(),
        chase: None,
        active_tropes: vec![],
        atmosphere: "desolate and windswept".to_string(),
        current_region: "flickering_reach".to_string(),
        discovered_regions: vec!["flickering_reach".to_string(), "rusted_bazaar".to_string()],
        discovered_routes: vec![],
        turn_manager: {
            let mut tm = TurnManager::new();
            tm.advance(); // simulate a round having completed (round 1→2)
            tm
        },
        last_saved_at: None,
        active_stakes: "The caravan's supplies are running low".to_string(),
        lore_established: vec!["The Flickering Reach was once a thriving trade hub".to_string()],
        turns_since_meaningful: 0,
        total_beats_fired: 2,
        campaign_maturity: sidequest_game::CampaignMaturity::Early,
        npc_registry: vec![],
        world_history: vec![],
        genie_wishes: vec![],
        axis_values: vec![],
        resource_state: HashMap::from([("Luck".to_string(), 0.75)]),
        ..GameSnapshot::default()
    }
}

// ============================================================================
// Source helpers — read dispatch source for structural verification
// ============================================================================

fn dispatch_source() -> String {
    let dispatch_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/dispatch/mod.rs");
    std::fs::read_to_string(&dispatch_path)
        .unwrap_or_else(|e| panic!("Failed to read dispatch/mod.rs: {e}"))
}

/// Extract a function body from source code by name.
/// Returns the text from `fn {name}(` to the next top-level function definition.
fn extract_fn_body<'a>(src: &'a str, fn_name: &str) -> &'a str {
    let needle = format!("fn {}(", fn_name);
    let fn_start = src.find(&needle)
        .unwrap_or_else(|| panic!("{} function must exist in dispatch/mod.rs", fn_name));
    let fn_body = &src[fn_start..];
    let fn_end = fn_body[1..].find("\nfn ")
        .or_else(|| fn_body[1..].find("\nasync fn "))
        .or_else(|| fn_body[1..].find("\npub fn "))
        .or_else(|| fn_body[1..].find("\npub(crate) fn "))
        .map(|i| i + 1)
        .unwrap_or(fn_body.len());
    &fn_body[..fn_end]
}

// ============================================================================
// AC-1: persist_game_state() must NOT call persistence().load() on save path
//
// The current code (lines 1264-1268) does:
//   match ctx.state.persistence().load(genre, world, player).await {
//       Ok(Some(saved)) => { let mut snapshot = saved.snapshot; ... }
//   }
// After this story, persist_game_state() should use ctx.snapshot directly.
// ============================================================================

#[test]
fn persist_game_state_does_not_load_before_save() {
    let src = dispatch_source();
    let persist_fn = extract_fn_body(&src, "persist_game_state");

    // The save path must NOT contain a load() call.
    // The source may span multiple lines (persistence()\n.load()), so strip whitespace.
    let persist_fn_compact: String = persist_fn.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        !persist_fn_compact.contains("persistence().load("),
        "persist_game_state() must NOT call persistence().load() on the save path. \
         The canonical GameSnapshot from DispatchContext should be saved directly \
         without loading the previous state first. Found load() call in persist_game_state()."
    );
}

#[test]
fn persist_game_state_does_not_merge_scattered_locals() {
    let src = dispatch_source();
    let persist_fn = extract_fn_body(&src, "persist_game_state");

    // The old code merges ~15 individual fields: snapshot.location = ...,
    // snapshot.turn_manager = ctx.turn_manager.clone(), etc.
    // After refactoring, these scattered merges should be gone — the snapshot
    // is already canonical and patched in-place during the turn.
    let scattered_merges = [
        "snapshot.turn_manager = ctx.turn_manager",
        "snapshot.npc_registry = ctx.npc_registry",
        "snapshot.genie_wishes = ctx.genie_wishes",
        "snapshot.combat = ctx.combat_state",
        "snapshot.chase = ctx.chase_state",
        "snapshot.discovered_regions = ctx.discovered_regions",
        "snapshot.active_tropes = ctx.trope_states",
        "snapshot.quest_log = ctx.quest_log",
    ];

    for merge in &scattered_merges {
        assert!(
            !persist_fn.contains(merge),
            "persist_game_state() still contains scattered local merge: '{}'. \
             After story 15-8, the canonical GameSnapshot in DispatchContext \
             should be saved directly — no field-by-field merging.",
            merge
        );
    }
}

// ============================================================================
// AC-1 (structural): DispatchContext must carry a `snapshot` field
//
// The current DispatchContext scatters ~37 individual field refs. After this
// story, it should include `snapshot: &'a mut GameSnapshot`.
// ============================================================================

#[test]
fn dispatch_context_has_snapshot_field() {
    let src = dispatch_source();

    // Find the DispatchContext struct definition
    let ctx_start = src.find("struct DispatchContext")
        .expect("DispatchContext struct must exist in dispatch/mod.rs");

    // Find the closing brace of the struct
    let ctx_body = &src[ctx_start..];
    let mut brace_depth = 0;
    let mut struct_end = ctx_body.len();
    for (i, ch) in ctx_body.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => {
                brace_depth -= 1;
                if brace_depth == 0 {
                    struct_end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    let struct_def = &ctx_body[..struct_end];

    assert!(
        struct_def.contains("snapshot"),
        "DispatchContext must include a 'snapshot' field (e.g., `pub snapshot: &'a mut GameSnapshot`). \
         Currently DispatchContext scatters state across ~37 individual field refs. \
         Story 15-8 requires a canonical GameSnapshot carried through the dispatch pipeline."
    );
}

// ============================================================================
// AC-2: OTEL watcher event — persistence.save_latency_ms
//
// After each save, persist_game_state() must emit a WatcherEvent with
// component="persistence" containing a "save_latency_ms" field.
// ============================================================================

#[test]
fn persist_game_state_emits_save_latency_otel_event() {
    let src = dispatch_source();

    let persist_fn = extract_fn_body(&src, "persist_game_state");

    assert!(
        persist_fn.contains("save_latency_ms"),
        "persist_game_state() must emit a WatcherEvent with 'save_latency_ms' field. \
         This OTEL event allows the GM panel to verify persistence optimization is working. \
         AC-2 requires: persistence.save_latency_ms on every turn."
    );
}

#[test]
fn persist_game_state_measures_elapsed_time() {
    let src = dispatch_source();

    let persist_fn = extract_fn_body(&src, "persist_game_state");

    // Must use Instant::now() or similar for timing measurement
    assert!(
        persist_fn.contains("Instant::now()") || persist_fn.contains("elapsed()"),
        "persist_game_state() must measure save duration using std::time::Instant. \
         The elapsed time feeds the save_latency_ms OTEL event."
    );
}

// ============================================================================
// AC-2 (structural): WatcherEvent component must be "persistence"
// ============================================================================

#[test]
fn persist_game_state_otel_uses_persistence_component() {
    let src = dispatch_source();

    let persist_fn = extract_fn_body(&src, "persist_game_state");

    // The WatcherEvent must use component: "persistence"
    assert!(
        persist_fn.contains(r#""persistence""#),
        "persist_game_state() WatcherEvent must use component=\"persistence\" \
         so the GM panel can filter and display persistence telemetry."
    );
}

// ============================================================================
// AC-3: Snapshot patch-in-place — multi-turn mutation preserves all fields
//
// Simulates what dispatch_player_action() should do: mutate the canonical
// GameSnapshot in-place across multiple turns, then verify all fields survive.
// ============================================================================

#[test]
fn snapshot_patch_in_place_preserves_fields_across_turns() {
    let mut snapshot = dispatch_snapshot();

    // Turn 1: combat starts, location changes
    snapshot.location = "Arena of the Damned".to_string();
    snapshot.combat.set_in_combat(true);
    snapshot.turn_manager.advance();
    snapshot.narrative_log.push(NarrativeEntry {
        timestamp: 2000,
        round: 2,
        author: "narrator".to_string(),
        content: "Combat erupts in the arena!".to_string(),
        tags: vec![],
        encounter_tags: vec![],
        speaker: None,
        entry_type: None,
    });

    // Turn 2: HP changes, inventory updated, NPC registered
    if let Some(ch) = snapshot.characters.first_mut() {
        ch.core.hp = 12;
        ch.core.inventory.add(test_item("arena_token", "Arena Token"), 20).unwrap();
    }
    snapshot.discovered_regions.push("arena_district".to_string());
    snapshot.turn_manager.advance();

    // Turn 3: combat ends, quest log updated
    snapshot.combat.set_in_combat(false);
    snapshot.quest_log.insert(
        "Arena Champion".to_string(),
        "Defeat the champion of the arena".to_string(),
    );
    snapshot.turn_manager.advance();

    // Verify ALL mutations survived patch-in-place
    assert_eq!(snapshot.location, "Arena of the Damned");
    assert!(snapshot.characters.first().unwrap().core.hp == 12);
    assert_eq!(snapshot.characters.first().unwrap().core.inventory.items.len(), 3);
    assert!(!snapshot.combat.in_combat());
    assert_eq!(snapshot.narrative_log.len(), 2);
    assert_eq!(snapshot.discovered_regions.len(), 3);
    assert!(snapshot.quest_log.contains_key("Arena Champion"));
    assert_eq!(snapshot.turn_manager.round(), 5); // started at round 2, +3 advances
}

// ============================================================================
// AC-3/AC-5: Save-only path — snapshot saved directly without prior load
//
// This tests the core optimization: save() works without a preceding load().
// The persistence layer should not require a load before save.
// ============================================================================

#[test]
fn save_without_prior_load_succeeds() {
    let store = SqliteStore::open_in_memory().unwrap();
    let snapshot = dispatch_snapshot();

    // Save directly — no load() call first
    let result = store.save(&snapshot);
    assert!(
        result.is_ok(),
        "Saving a snapshot without prior load must succeed. \
         persist_game_state() depends on this for the save-only hot path."
    );
}

#[test]
fn save_without_prior_load_then_load_recovers_all_fields() {
    let store = SqliteStore::open_in_memory().unwrap();
    let snapshot = dispatch_snapshot();

    // Save directly (no prior load)
    store.save(&snapshot).expect("save should succeed");

    // Load and verify all dispatch-relevant fields survived
    let loaded = store.load().expect("load should succeed");
    let loaded = loaded.expect("saved session should exist");
    let loaded_snap = loaded.snapshot;

    assert_eq!(loaded_snap.genre_slug, "mutant_wasteland");
    assert_eq!(loaded_snap.world_slug, "flickering_reach");
    assert_eq!(loaded_snap.location, "Rusted Bazaar");
    assert_eq!(loaded_snap.time_of_day, "dusk");
    assert_eq!(loaded_snap.atmosphere, "desolate and windswept");
    assert_eq!(loaded_snap.current_region, "flickering_reach");

    // Character state
    let ch = loaded_snap.characters.first().expect("character must survive save/load");
    assert_eq!(ch.core.hp, 18);
    assert_eq!(ch.core.max_hp, 25);
    assert_eq!(ch.core.level, 3);
    assert_eq!(ch.core.xp, 450);
    assert_eq!(ch.core.inventory.items.len(), 2);

    // World state
    assert_eq!(loaded_snap.discovered_regions.len(), 2);
    assert!(loaded_snap.quest_log.contains_key("Find the Caravan"));
    assert_eq!(loaded_snap.narrative_log.len(), 1);
    assert_eq!(loaded_snap.lore_established.len(), 1);

    // Turn manager — round survives save/load
    assert_eq!(loaded_snap.turn_manager.round(), snapshot.turn_manager.round());

    // Resource state
    assert_eq!(*loaded_snap.resource_state.get("Luck").unwrap_or(&0.0), 0.75);
}

// ============================================================================
// AC-3: Multi-turn save/load cycle — patched snapshot persists correctly
// ============================================================================

#[test]
fn multi_turn_patch_then_save_preserves_mutations() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mut snapshot = dispatch_snapshot();

    // Simulate 3 turns of patching
    snapshot.location = "Scorched Outpost".to_string();
    snapshot.turn_manager.advance();
    if let Some(ch) = snapshot.characters.first_mut() {
        ch.core.hp = 5;
        ch.core.level = 4;
    }
    snapshot.narrative_log.push(NarrativeEntry {
        timestamp: 3000,
        round: 3,
        author: "narrator".to_string(),
        content: "The outpost smolders.".to_string(),
        tags: vec![],
        encounter_tags: vec![],
        speaker: None,
        entry_type: None,
    });

    // Save the patched snapshot directly
    store.save(&snapshot).expect("save patched snapshot");

    // Load and verify mutations persisted
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.snapshot.location, "Scorched Outpost");
    assert_eq!(loaded.snapshot.characters.first().unwrap().core.hp, 5);
    assert_eq!(loaded.snapshot.characters.first().unwrap().core.level, 4);
    assert_eq!(loaded.snapshot.narrative_log.len(), 2);
}

// ============================================================================
// AC-4: Session restore on reconnect — load path must still work
//
// dispatch_connect() still needs to load from SQLite when a player reconnects.
// This path is infrequent but critical.
// ============================================================================

#[test]
fn session_restore_loads_from_sqlite() {
    let store = SqliteStore::open_in_memory().unwrap();
    let snapshot = dispatch_snapshot();

    // Initial save (simulates a previous session)
    store.save(&snapshot).expect("initial save");

    // Simulate reconnect: load from SQLite
    let restored = store.load().expect("load should succeed");
    assert!(
        restored.is_some(),
        "Session restore must find the saved session. \
         dispatch_connect() depends on this for reconnection."
    );

    let restored = restored.unwrap();
    assert_eq!(restored.snapshot.genre_slug, snapshot.genre_slug);
    assert_eq!(restored.snapshot.world_slug, snapshot.world_slug);
    assert_eq!(restored.snapshot.characters.len(), snapshot.characters.len());
}

#[test]
fn session_restore_after_multi_save_returns_latest() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mut snapshot = dispatch_snapshot();

    // Save v1
    store.save(&snapshot).expect("save v1");

    // Mutate and save v2
    snapshot.location = "Updated Location".to_string();
    snapshot.turn_manager.advance();
    store.save(&snapshot).expect("save v2");

    // Restore should return v2
    let restored = store.load().unwrap().unwrap();
    assert_eq!(
        restored.snapshot.location, "Updated Location",
        "Session restore must return the most recent save, not an earlier version."
    );
}

// ============================================================================
// AC-1 (structural): persist_game_state saves ctx.snapshot directly
//
// The function should reference ctx.snapshot (the canonical state) rather
// than constructing a new snapshot from scattered locals.
// ============================================================================

#[test]
fn persist_game_state_uses_ctx_snapshot() {
    let src = dispatch_source();

    let persist_fn = extract_fn_body(&src, "persist_game_state");

    // Must reference ctx.snapshot (the canonical snapshot carried in DispatchContext)
    assert!(
        persist_fn.contains("ctx.snapshot") || persist_fn.contains("&ctx.snapshot"),
        "persist_game_state() must save ctx.snapshot directly. \
         The canonical GameSnapshot in DispatchContext is the source of truth — \
         no loading, no merging scattered locals."
    );
}

// ============================================================================
// AC-6: Error handling — save failure must NOT silently swallow errors
// ============================================================================

#[test]
fn persist_game_state_has_error_handling_on_save() {
    let src = dispatch_source();

    let persist_fn = extract_fn_body(&src, "persist_game_state");

    // Must have error logging on the save path (warn! or error!)
    assert!(
        persist_fn.contains("tracing::warn!") || persist_fn.contains("tracing::error!"),
        "persist_game_state() must log errors on save failure. \
         No silent fallbacks — the CLAUDE.md rules require loud failure."
    );
}

// ============================================================================
// Wiring test: dispatch_player_action call site populates snapshot
//
// lib.rs constructs DispatchContext before calling dispatch_player_action.
// After this story, it must include a snapshot field.
// ============================================================================

#[test]
fn lib_dispatch_context_construction_includes_snapshot() {
    let lib_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/lib.rs");
    let src = std::fs::read_to_string(&lib_path)
        .unwrap_or_else(|e| panic!("Failed to read lib.rs: {e}"));

    // Find the DispatchContext construction in the PlayerAction handler
    // It should include a `snapshot:` field
    let ctx_construction = src.find("DispatchContext {")
        .expect("DispatchContext construction must exist in lib.rs");

    // Get a reasonable chunk after the construction start
    let construction_body = &src[ctx_construction..];
    let end = construction_body.find("}.await")
        .or_else(|| construction_body.find("}\n").map(|i| i + 1))
        .unwrap_or(construction_body.len().min(2000));
    let construction = &construction_body[..end];

    assert!(
        construction.contains("snapshot"),
        "DispatchContext construction in lib.rs must include a 'snapshot' field. \
         The canonical GameSnapshot must be passed into dispatch_player_action(). \
         Currently lib.rs constructs DispatchContext with ~37 individual field refs — \
         story 15-8 adds the canonical snapshot."
    );
}

// ============================================================================
// Rule coverage: Rust lang-review checklist
// ============================================================================

// Rule #4: Tracing coverage — error paths must have tracing calls
#[test]
fn persist_game_state_traces_empty_slugs_early_return() {
    let src = dispatch_source();

    let persist_fn = extract_fn_body(&src, "persist_game_state");

    // The early return for empty genre/world slugs should log, not silently return
    if persist_fn.contains("is_empty()") {
        // If there's an early-return guard on empty slugs, it should at least trace
        assert!(
            persist_fn.contains("tracing::debug!") ||
            persist_fn.contains("tracing::warn!") ||
            persist_fn.contains("tracing::info!") ||
            persist_fn.contains("tracing::error!"),
            "persist_game_state() early return for empty slugs should have a tracing call. \
             Rule #4: error/guard paths must have tracing coverage."
        );
    }
}

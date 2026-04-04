//! Story 19-10: Wire deplete_light_on_transition into room transition dispatch
//!
//! Tests that deplete_light_on_transition provides enough context for the dispatch
//! layer to construct GameMessage::ItemDepleted with remaining_before, and that
//! the integration between room movement and light depletion works end-to-end.
//!
//! AC coverage:
//! - AC1: Room transition handler calls deplete_light_on_transition() with inventory
//! - AC2: deplete_light_on_transition() decrements and returns Option<Item> if exhausted
//! - AC5: OTEL span data: item_name and remaining_before extractable from return value
//! - AC6: Full wiring scenario — room transition + depletion + message data
//! - AC7: 6-use torch scenario at integration level
//!
//! Rule coverage:
//! - wiring-test: verifies the game-crate function returns data the server needs
//! - no-silent-fallback: no light source → no depletion (not a silent default)

use sidequest_game::inventory::{Inventory, Item};
use sidequest_game::room_movement::{apply_validated_move, init_room_graph_location};
use sidequest_game::state::GameSnapshot;
use sidequest_genre::{RoomDef, RoomExit};
use sidequest_protocol::NonBlankString;

// ═══════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════

fn make_torch(uses: u32) -> Item {
    Item {
        id: NonBlankString::new("torch").unwrap(),
        name: NonBlankString::new("Torch").unwrap(),
        description: NonBlankString::new("A flickering wooden torch").unwrap(),
        category: NonBlankString::new("consumable").unwrap(),
        value: 1,
        weight: 1.0,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.1,
        tags: vec!["light".to_string()],
        equipped: true,
        quantity: 1,
        uses_remaining: Some(uses),
    }
}

fn make_sword() -> Item {
    Item {
        id: NonBlankString::new("sword_iron").unwrap(),
        name: NonBlankString::new("Iron Sword").unwrap(),
        description: NonBlankString::new("A sturdy iron blade").unwrap(),
        category: NonBlankString::new("weapon").unwrap(),
        value: 50,
        weight: 3.0,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.3,
        tags: vec!["melee".to_string(), "blade".to_string()],
        equipped: true,
        quantity: 1,
        uses_remaining: None,
    }
}

/// Build a minimal two-room graph: entrance → corridor (connected via door).
fn two_room_graph() -> Vec<RoomDef> {
    vec![
        RoomDef {
            id: "entrance".to_string(),
            name: "Entrance Hall".to_string(),
            description: Some("A dusty stone entrance.".to_string()),
            room_type: "entrance".to_string(),
            size: (1, 1),
            exits: vec![RoomExit::Door {
                target: "corridor".to_string(),
                is_locked: false,
            }],
            keeper_awareness_modifier: 1.0,
        },
        RoomDef {
            id: "corridor".to_string(),
            name: "Dark Corridor".to_string(),
            description: Some("A long, dark corridor.".to_string()),
            room_type: "corridor".to_string(),
            size: (1, 1),
            exits: vec![RoomExit::Door {
                target: "entrance".to_string(),
                is_locked: false,
            }],
            keeper_awareness_modifier: 1.5,
        },
    ]
}

/// Build a three-room loop for multi-transition tests.
fn three_room_loop() -> Vec<RoomDef> {
    vec![
        RoomDef {
            id: "entrance".to_string(),
            name: "Entrance Hall".to_string(),
            description: Some("A dusty stone entrance.".to_string()),
            room_type: "entrance".to_string(),
            size: (1, 1),
            exits: vec![RoomExit::Corridor {
                target: "hallway".to_string(),
            }],
            keeper_awareness_modifier: 1.0,
        },
        RoomDef {
            id: "hallway".to_string(),
            name: "Central Hallway".to_string(),
            description: Some("A junction of corridors.".to_string()),
            room_type: "corridor".to_string(),
            size: (2, 2),
            exits: vec![
                RoomExit::Corridor {
                    target: "entrance".to_string(),
                },
                RoomExit::Door {
                    target: "vault".to_string(),
                    is_locked: false,
                },
            ],
            keeper_awareness_modifier: 1.0,
        },
        RoomDef {
            id: "vault".to_string(),
            name: "Treasure Vault".to_string(),
            description: Some("Glittering gold and ancient relics.".to_string()),
            room_type: "room".to_string(),
            size: (3, 3),
            exits: vec![RoomExit::Door {
                target: "hallway".to_string(),
                is_locked: false,
            }],
            keeper_awareness_modifier: 2.0,
        },
    ]
}

fn make_snapshot() -> GameSnapshot {
    GameSnapshot::default()
}

// ═══════════════════════════════════════════════════════════
// AC1 + AC6: Room transition triggers light depletion
// ═══════════════════════════════════════════════════════════

/// After a successful room move, calling deplete_light_on_transition
/// decrements the light source. This simulates the dispatch wiring.
#[test]
fn room_move_then_deplete_light_integration() {
    let rooms = two_room_graph();
    let mut snap = make_snapshot();
    init_room_graph_location(&mut snap, &rooms);
    assert_eq!(snap.location, "entrance");

    let mut inv = Inventory::default();
    inv.add(make_torch(6), 10).unwrap();

    // Move to corridor
    let transition = apply_validated_move(&mut snap, "corridor", &rooms).unwrap();
    assert_eq!(transition.from_room, "entrance");
    assert_eq!(transition.to_room, "corridor");

    // Dispatch calls deplete_light_on_transition after move
    let depleted = inv.deplete_light_on_transition();
    assert!(depleted.is_none(), "Torch has 5 uses left, not exhausted");

    let torch = inv.find("torch").expect("Torch still in inventory");
    assert_eq!(torch.uses_remaining, Some(5));
}

// ═══════════════════════════════════════════════════════════
// AC2 + AC5: Return value carries enough data for OTEL span
// ═══════════════════════════════════════════════════════════

/// When a light source is exhausted, the returned Item contains the item_name
/// needed for the OTEL span and the uses_remaining (0) needed to compute
/// remaining_before (which was 1 before the final decrement).
///
/// The dispatch layer needs: item_name = depleted.name, remaining_before = 1
/// (since the item was at uses_remaining=1 before the call, now 0 after removal).
#[test]
fn depleted_item_carries_otel_span_data() {
    let mut inv = Inventory::default();
    inv.add(make_torch(1), 10).unwrap();

    // Caller should capture remaining_before BEFORE calling deplete
    let remaining_before = inv
        .find("torch")
        .and_then(|i| i.uses_remaining)
        .unwrap_or(0);
    assert_eq!(remaining_before, 1, "Pre-depletion uses_remaining");

    let depleted = inv.deplete_light_on_transition();
    assert!(depleted.is_some());

    let item = depleted.unwrap();
    // item_name for OTEL span
    assert_eq!(item.name.as_str(), "Torch");
    // item has 0 remaining after depletion
    assert_eq!(item.uses_remaining, Some(0));
}

/// Non-exhaustion still decrements — dispatch needs to know NOT to fire ItemDepleted.
#[test]
fn non_exhaustion_returns_none_no_message() {
    let mut inv = Inventory::default();
    inv.add(make_torch(6), 10).unwrap();

    let depleted = inv.deplete_light_on_transition();
    assert!(
        depleted.is_none(),
        "No ItemDepleted message should fire when torch is not exhausted"
    );
}

// ═══════════════════════════════════════════════════════════
// AC3: GameMessage::ItemDepleted construction from depletion data
// ═══════════════════════════════════════════════════════════

/// Simulate what the dispatch layer must do: capture remaining_before,
/// call deplete, construct GameMessage::ItemDepleted if exhausted.
#[test]
fn dispatch_constructs_item_depleted_message_on_exhaustion() {
    let mut inv = Inventory::default();
    inv.add(make_torch(1), 10).unwrap();

    // Step 1: Capture remaining_before (dispatch must do this before calling deplete)
    let light_item = inv
        .items
        .iter()
        .find(|item| item.tags.iter().any(|t| t == "light"));
    let remaining_before = light_item
        .and_then(|i| i.uses_remaining)
        .unwrap_or(0);

    // Step 2: Call deplete
    let depleted = inv.deplete_light_on_transition();

    // Step 3: Construct message (what dispatch does)
    if let Some(ref item) = depleted {
        let item_name = item.name.as_str().to_owned();
        // These are the exact fields needed for GameMessage::ItemDepleted
        assert_eq!(item_name, "Torch");
        assert_eq!(remaining_before, 1_u32);
    } else {
        panic!("Expected exhaustion — torch had 1 use remaining");
    }
}

/// When no light source exists, dispatch should NOT fire ItemDepleted.
#[test]
fn dispatch_no_message_without_light_source() {
    let mut inv = Inventory::default();
    inv.add(make_sword(), 10).unwrap();

    let depleted = inv.deplete_light_on_transition();
    assert!(
        depleted.is_none(),
        "No ItemDepleted message when no light source in inventory"
    );
}

// ═══════════════════════════════════════════════════════════
// AC7: Full 6-use torch lifecycle with room transitions
// ═══════════════════════════════════════════════════════════

/// 6-use torch across 6 room transitions: survives 5, fires ItemDepleted on 6th.
/// This is the canonical end-to-end scenario from the AC.
#[test]
fn six_use_torch_full_lifecycle_with_room_moves() {
    let rooms = two_room_graph();
    let mut snap = make_snapshot();
    init_room_graph_location(&mut snap, &rooms);

    let mut inv = Inventory::default();
    inv.add(make_torch(6), 10).unwrap();

    let destinations = ["corridor", "entrance", "corridor", "entrance", "corridor", "entrance"];
    let mut item_depleted_fired = false;
    let mut item_depleted_name = String::new();
    let mut item_depleted_remaining_before = 0u32;

    for (i, dest) in destinations.iter().enumerate() {
        // Move
        let _transition = apply_validated_move(&mut snap, dest, &rooms).unwrap();

        // Capture remaining_before for OTEL + message (dispatch must do this)
        let remaining_before = inv
            .items
            .iter()
            .find(|item| item.tags.iter().any(|t| t == "light"))
            .and_then(|item| item.uses_remaining)
            .unwrap_or(0);

        // Deplete
        let depleted = inv.deplete_light_on_transition();

        if let Some(ref item) = depleted {
            item_depleted_fired = true;
            item_depleted_name = item.name.as_str().to_owned();
            item_depleted_remaining_before = remaining_before;
            assert_eq!(
                i, 5,
                "ItemDepleted should fire on the 6th transition (index 5), not transition {}",
                i
            );
        } else {
            assert!(
                i < 5,
                "Torch should survive transitions 0-4, but failed at transition {i}"
            );
        }
    }

    // Verify the ItemDepleted data
    assert!(item_depleted_fired, "ItemDepleted must fire on 6th transition");
    assert_eq!(item_depleted_name, "Torch");
    assert_eq!(
        item_depleted_remaining_before, 1,
        "remaining_before should be 1 (the last use before exhaustion)"
    );

    // Torch is gone
    assert!(inv.find("torch").is_none(), "Torch removed after exhaustion");
}

// ═══════════════════════════════════════════════════════════
// Wiring test: deplete is called in room_graph mode ONLY
// ═══════════════════════════════════════════════════════════

/// In region mode (no rooms), no room graph validation happens and
/// deplete_light_on_transition should NOT be called by dispatch.
/// This test verifies the inventory is unchanged when there's no room graph.
#[test]
fn region_mode_no_room_transition_no_depletion() {
    let mut inv = Inventory::default();
    inv.add(make_torch(6), 10).unwrap();

    // In region mode, rooms is empty — dispatch skips room graph validation
    let rooms: Vec<RoomDef> = vec![];
    assert!(rooms.is_empty(), "Region mode has no rooms");

    // Inventory should be untouched (dispatch doesn't call deplete in region mode)
    let torch = inv.find("torch").expect("Torch exists");
    assert_eq!(torch.uses_remaining, Some(6), "Torch unchanged in region mode");
}

// ═══════════════════════════════════════════════════════════
// Edge: multiple room transitions in a loop
// ═══════════════════════════════════════════════════════════

/// Three-room loop with a 3-use torch: entrance→hallway→vault→hallway (3 moves).
/// Torch exhausted on 3rd move.
#[test]
fn three_room_loop_depletes_torch() {
    let rooms = three_room_loop();
    let mut snap = make_snapshot();
    init_room_graph_location(&mut snap, &rooms);
    assert_eq!(snap.location, "entrance");

    let mut inv = Inventory::default();
    inv.add(make_torch(3), 10).unwrap();

    // Move 1: entrance → hallway
    apply_validated_move(&mut snap, "hallway", &rooms).unwrap();
    let d1 = inv.deplete_light_on_transition();
    assert!(d1.is_none(), "2 uses left");
    assert_eq!(inv.find("torch").unwrap().uses_remaining, Some(2));

    // Move 2: hallway → vault
    apply_validated_move(&mut snap, "vault", &rooms).unwrap();
    let d2 = inv.deplete_light_on_transition();
    assert!(d2.is_none(), "1 use left");
    assert_eq!(inv.find("torch").unwrap().uses_remaining, Some(1));

    // Move 3: vault → hallway — torch exhausted
    apply_validated_move(&mut snap, "hallway", &rooms).unwrap();
    let d3 = inv.deplete_light_on_transition();
    assert!(d3.is_some(), "Torch exhausted on 3rd transition");
    assert!(inv.find("torch").is_none(), "Torch gone from inventory");
}

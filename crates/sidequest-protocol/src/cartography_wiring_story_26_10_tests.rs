//! Story 26-10: Wire map cartography data through dispatch to UI
//!
//! RED tests — these verify that MAP_UPDATE carries cartography metadata
//! from genre packs. Currently failing because MapUpdatePayload has
//! `deny_unknown_fields` and no `cartography` field.

use serde_json::json;

/// AC-1: MAP_UPDATE should carry cartography metadata.
/// Fails: `deny_unknown_fields` rejects the `cartography` key.
#[test]
fn map_update_deserializes_with_cartography_metadata() {
    let json_val = json!({
        "type": "MAP_UPDATE",
        "payload": {
            "current_location": "Dark Cave",
            "region": "Shadowlands",
            "explored": [],
            "cartography": {
                "navigation_mode": "region",
                "starting_region": "Shadowlands",
                "regions": {
                    "Shadowlands": {
                        "name": "Shadowlands",
                        "description": "Dark and foreboding lands"
                    }
                },
                "routes": []
            }
        },
        "player_id": "p1"
    });
    let msg: crate::GameMessage = serde_json::from_value(json_val)
        .expect("MAP_UPDATE with cartography should deserialize");
    match msg {
        crate::GameMessage::MapUpdate { payload, .. } => {
            let payload_json = serde_json::to_value(&payload).unwrap();
            let carto = payload_json
                .get("cartography")
                .expect("payload should contain cartography field");
            assert_eq!(
                carto.get("navigation_mode").and_then(|v| v.as_str()),
                Some("region"),
                "cartography should carry navigation_mode"
            );
            assert_eq!(
                carto.get("starting_region").and_then(|v| v.as_str()),
                Some("Shadowlands"),
                "cartography should carry starting_region"
            );
            assert!(
                carto.get("regions").and_then(|v| v.as_object()).is_some(),
                "cartography should carry regions map"
            );
        }
        _ => panic!("Expected MapUpdate variant"),
    }
}

/// AC-2: Cartography regions include name, description, and adjacency.
/// Fails: same deny_unknown_fields rejection.
#[test]
fn cartography_regions_carry_full_metadata() {
    let json_val = json!({
        "type": "MAP_UPDATE",
        "payload": {
            "current_location": "Town Square",
            "region": "Eldergrove",
            "explored": [],
            "cartography": {
                "navigation_mode": "region",
                "starting_region": "Eldergrove",
                "regions": {
                    "Eldergrove": {
                        "name": "Eldergrove",
                        "description": "Ancient forest with towering oaks",
                        "adjacent": ["Shadowlands", "Brightmoor"]
                    },
                    "Shadowlands": {
                        "name": "Shadowlands",
                        "description": "Dark and twisted",
                        "adjacent": ["Eldergrove"]
                    }
                },
                "routes": [
                    {
                        "name": "Forest Trail",
                        "description": "A winding path through old growth",
                        "from_id": "Eldergrove",
                        "to_id": "Shadowlands"
                    }
                ]
            }
        },
        "player_id": "p1"
    });
    let msg: crate::GameMessage = serde_json::from_value(json_val)
        .expect("MAP_UPDATE with full region data should deserialize");
    match msg {
        crate::GameMessage::MapUpdate { payload, .. } => {
            let payload_json = serde_json::to_value(&payload).unwrap();
            let carto = payload_json.get("cartography").expect("cartography present");
            let regions = carto.get("regions").and_then(|v| v.as_object()).unwrap();
            assert_eq!(regions.len(), 2, "should have two regions");
            let eldergrove = regions.get("Eldergrove").unwrap();
            assert_eq!(
                eldergrove.get("name").and_then(|v| v.as_str()),
                Some("Eldergrove")
            );
            let adjacent = eldergrove
                .get("adjacent")
                .and_then(|v| v.as_array())
                .expect("region should have adjacent list");
            assert_eq!(adjacent.len(), 2);
            let routes = carto.get("routes").and_then(|v| v.as_array()).unwrap();
            assert_eq!(routes.len(), 1);
            assert_eq!(
                routes[0].get("from_id").and_then(|v| v.as_str()),
                Some("Eldergrove")
            );
        }
        _ => panic!("Expected MapUpdate variant"),
    }
}

/// AC-3: Cartography supports room_graph navigation mode.
/// Verifies room-graph mode serialization in the cartography payload.
#[test]
fn cartography_supports_room_graph_navigation_mode() {
    let json_val = json!({
        "type": "MAP_UPDATE",
        "payload": {
            "current_location": "Entry Hall",
            "region": "Dungeon Level 1",
            "explored": [],
            "cartography": {
                "navigation_mode": "room_graph",
                "starting_region": "entry_hall",
                "regions": {},
                "routes": []
            }
        },
        "player_id": "p1"
    });
    let msg: crate::GameMessage = serde_json::from_value(json_val)
        .expect("MAP_UPDATE with room_graph mode should deserialize");
    match msg {
        crate::GameMessage::MapUpdate { payload, .. } => {
            let payload_json = serde_json::to_value(&payload).unwrap();
            let carto = payload_json.get("cartography").expect("cartography present");
            assert_eq!(
                carto.get("navigation_mode").and_then(|v| v.as_str()),
                Some("room_graph"),
                "navigation_mode should be room_graph"
            );
        }
        _ => panic!("Expected MapUpdate variant"),
    }
}

/// AC-4: Cartography is optional — backward compat.
/// MAP_UPDATE without cartography should still deserialize fine.
/// This test should PASS — it guards against regressions.
#[test]
fn map_update_without_cartography_still_deserializes() {
    let json_val = json!({
        "type": "MAP_UPDATE",
        "payload": {
            "current_location": "Dark Cave",
            "region": "Shadowlands",
            "explored": []
        },
        "player_id": "p1"
    });
    let msg: crate::GameMessage = serde_json::from_value(json_val)
        .expect("MAP_UPDATE without cartography should still work");
    match msg {
        crate::GameMessage::MapUpdate { payload, .. } => {
            assert_eq!(payload.current_location, "Dark Cave");
            assert_eq!(payload.region, "Shadowlands");
            // Cartography should be None when not provided
            let payload_json = serde_json::to_value(&payload).unwrap();
            assert!(
                payload_json.get("cartography").is_none()
                    || payload_json.get("cartography").unwrap().is_null(),
                "cartography should be absent or null when not provided"
            );
        }
        _ => panic!("Expected MapUpdate variant"),
    }
}

/// AC-5: Round-trip — MAP_UPDATE with cartography survives serialize→deserialize.
#[test]
fn map_update_with_cartography_round_trips() {
    let json_val = json!({
        "type": "MAP_UPDATE",
        "payload": {
            "current_location": "Market District",
            "region": "Brightmoor",
            "explored": [
                {
                    "name": "Market District",
                    "x": 5,
                    "y": 3,
                    "type": "settlement",
                    "connections": ["Harbor"]
                }
            ],
            "cartography": {
                "navigation_mode": "region",
                "starting_region": "Brightmoor",
                "regions": {
                    "Brightmoor": {
                        "name": "Brightmoor",
                        "description": "Prosperous trading city"
                    }
                },
                "routes": []
            }
        },
        "player_id": "p2"
    });
    let msg: crate::GameMessage = serde_json::from_value(json_val.clone())
        .expect("should deserialize with cartography");
    let re_serialized = serde_json::to_value(&msg).unwrap();
    let round_tripped: crate::GameMessage =
        serde_json::from_value(re_serialized.clone()).expect("should survive round-trip");
    assert_eq!(msg, round_tripped, "round-trip should be lossless");
    // Verify cartography survived
    let payload = re_serialized.get("payload").unwrap();
    assert!(
        payload.get("cartography").is_some(),
        "cartography should survive serialization"
    );
}

/// WIRING: Verify MapUpdatePayload is re-exported from crate root.
/// Integration consumers (dispatch) import from sidequest_protocol::*.
#[test]
fn map_update_payload_accessible_from_crate_root() {
    // If this compiles, the type is properly re-exported.
    let _payload = crate::MapUpdatePayload {
        current_location: "test".into(),
        region: "test".into(),
        explored: vec![],
        fog_bounds: None,
        cartography: None,
    };
}

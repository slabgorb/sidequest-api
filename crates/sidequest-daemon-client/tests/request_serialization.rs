//! RED phase tests: Verify daemon request JSON structure.
//!
//! These tests encode the JSON-RPC request contract:
//! `{"id": "<uuid>", "method": "<name>", "params": {...}}`

use sidequest_daemon_client::{build_request_json, RenderParams, WarmUpParams};

// ---------------------------------------------------------------------------
// Render request
// ---------------------------------------------------------------------------

#[test]
fn render_request_has_method_field_set_to_render() {
    let params = RenderParams::default();
    let json = build_request_json("render", &params);
    assert_eq!(
        json["method"], "render",
        "render request must have method: 'render'"
    );
}

#[test]
fn render_request_has_uuid_id() {
    let params = RenderParams::default();
    let json = build_request_json("render", &params);
    let id_str = json["id"].as_str().expect("request id must be a string");
    let parsed = uuid::Uuid::parse_str(id_str);
    assert!(
        parsed.is_ok(),
        "request id must be a valid UUID, got: {id_str}"
    );
    assert_eq!(
        parsed.unwrap().get_version(),
        Some(uuid::Version::Random),
        "request id must be UUID v4"
    );
}

#[test]
fn render_request_params_contains_prompt() {
    let params = RenderParams::default();
    let json = build_request_json("render", &params);
    assert!(
        json["params"].get("prompt").is_some(),
        "render params must include 'prompt' field"
    );
}

#[test]
fn render_request_params_contains_art_style() {
    let params = RenderParams::default();
    let json = build_request_json("render", &params);
    assert!(
        json["params"].get("art_style").is_some(),
        "render params must include 'art_style' field"
    );
}

// ---------------------------------------------------------------------------
// Warm-up request
// ---------------------------------------------------------------------------

#[test]
fn warm_up_request_has_method_field_set_to_warm_up() {
    let params = WarmUpParams::default();
    let json = build_request_json("warm_up", &params);
    assert_eq!(
        json["method"], "warm_up",
        "warm_up request must have method: 'warm_up'"
    );
}

#[test]
fn warm_up_request_params_contains_worker() {
    let params = WarmUpParams::default();
    let json = build_request_json("warm_up", &params);
    assert!(
        json["params"].get("worker").is_some(),
        "warm_up params must include 'worker' field"
    );
}

// ---------------------------------------------------------------------------
// Ping request
// ---------------------------------------------------------------------------

#[test]
fn ping_request_has_method_field_set_to_ping() {
    let json = build_request_json("ping", &serde_json::json!({}));
    assert_eq!(
        json["method"], "ping",
        "ping request must have method: 'ping'"
    );
}

// ---------------------------------------------------------------------------
// ID uniqueness
// ---------------------------------------------------------------------------

#[test]
fn each_request_gets_a_unique_id() {
    let params = RenderParams::default();
    let json_a = build_request_json("render", &params);
    let json_b = build_request_json("render", &params);
    assert_ne!(
        json_a["id"], json_b["id"],
        "two requests must have different IDs"
    );
}

// ---------------------------------------------------------------------------
// Envelope structure
// ---------------------------------------------------------------------------

#[test]
fn request_envelope_has_exactly_three_top_level_keys() {
    let params = RenderParams::default();
    let json = build_request_json("render", &params);
    let obj = json.as_object().expect("request must be a JSON object");
    assert_eq!(
        obj.len(),
        3,
        "request envelope must have exactly id, method, params — got keys: {:?}",
        obj.keys().collect::<Vec<_>>()
    );
    assert!(obj.contains_key("id"), "missing 'id' key");
    assert!(obj.contains_key("method"), "missing 'method' key");
    assert!(obj.contains_key("params"), "missing 'params' key");
}

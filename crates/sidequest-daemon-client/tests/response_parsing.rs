//! RED phase tests: Parse daemon JSON responses.
//!
//! Response contract: `{"id": "<uuid>", "result": {...}}` on success,
//! or `{"id": "<uuid>", "error": {"code": N, "message": "..."}` on failure.

use sidequest_daemon_client::{DaemonResponse, ErrorPayload, RenderResult, StatusResult};

// ---------------------------------------------------------------------------
// Successful response parsing
// ---------------------------------------------------------------------------

#[test]
fn parse_render_success_response() {
    let json = r#"{
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "result": {
            "image_url": "/render/abc123.png",
            "generation_ms": 1500
        }
    }"#;
    let resp: DaemonResponse = serde_json::from_str(json)
        .expect("should parse render success response");
    assert!(resp.result.is_some(), "success response must have 'result'");
    assert!(resp.error.is_none(), "success response must not have 'error'");
}

#[test]
fn render_result_preserves_image_url() {
    let json = r#"{"image_url": "/render/abc123.png", "generation_ms": 1500}"#;
    let result: RenderResult = serde_json::from_str(json)
        .expect("should parse render result");
    let round_trip = serde_json::to_value(&result).unwrap();
    assert_eq!(
        round_trip.get("image_url").and_then(|v| v.as_str()),
        Some("/render/abc123.png"),
        "RenderResult must preserve image_url field through round-trip"
    );
}

#[test]
fn render_result_preserves_generation_ms() {
    let json = r#"{"image_url": "/render/abc123.png", "generation_ms": 1500}"#;
    let result: RenderResult = serde_json::from_str(json)
        .expect("should parse render result");
    let round_trip = serde_json::to_value(&result).unwrap();
    assert_eq!(
        round_trip.get("generation_ms").and_then(|v| v.as_u64()),
        Some(1500),
        "RenderResult must preserve generation_ms field through round-trip"
    );
}

#[test]
fn status_result_preserves_status_field() {
    let json = r#"{"status": "ready", "workers": 2}"#;
    let result: StatusResult = serde_json::from_str(json)
        .expect("should parse status result");
    let round_trip = serde_json::to_value(&result).unwrap();
    assert_eq!(
        round_trip.get("status").and_then(|v| v.as_str()),
        Some("ready"),
        "StatusResult must preserve status field through round-trip"
    );
}

// ---------------------------------------------------------------------------
// Error response parsing
// ---------------------------------------------------------------------------

#[test]
fn parse_daemon_error_response() {
    let json = r#"{
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "error": {
            "code": -32000,
            "message": "GPU out of memory"
        }
    }"#;
    let resp: DaemonResponse = serde_json::from_str(json)
        .expect("should parse error response");
    assert!(resp.result.is_none(), "error response must not have 'result'");
    let err = resp.error.as_ref().expect("error response must have 'error'");
    assert_eq!(err.code, -32000);
    assert_eq!(err.message, "GPU out of memory");
}

#[test]
fn error_payload_round_trips() {
    let payload = ErrorPayload {
        code: -32600,
        message: "invalid request".into(),
    };
    let json = serde_json::to_string(&payload).unwrap();
    let parsed: ErrorPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.code, -32600);
    assert_eq!(parsed.message, "invalid request");
}

// ---------------------------------------------------------------------------
// Response ID tracking
// ---------------------------------------------------------------------------

#[test]
fn response_preserves_request_id() {
    let json = r#"{
        "id": "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d",
        "result": {}
    }"#;
    let resp: DaemonResponse = serde_json::from_str(json)
        .expect("should parse response");
    assert_eq!(
        resp.id.to_string(),
        "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d",
        "response must preserve the request UUID"
    );
}

// ---------------------------------------------------------------------------
// Malformed responses
// ---------------------------------------------------------------------------

#[test]
fn reject_response_missing_id() {
    let json = r#"{"result": {"status": "ok"}}"#;
    let result: Result<DaemonResponse, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "response without 'id' field must fail to parse"
    );
}

#[test]
fn reject_completely_malformed_json() {
    let json = "this is not json at all";
    let result: Result<DaemonResponse, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "non-JSON input must fail to parse"
    );
}

#[test]
fn reject_json_array_as_response() {
    let json = r#"[1, 2, 3]"#;
    let result: Result<DaemonResponse, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "JSON array must not parse as DaemonResponse"
    );
}

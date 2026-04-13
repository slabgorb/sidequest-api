//! Story 37-5: Embed error handling and response schema tests.
//!
//! Playtest 2 (2026-04-12): /embed returned "Unknown error".
//!
//! These tests verify:
//! 1. Daemon error responses with string codes deserialize correctly
//! 2. The StatusResult type matches what the daemon actually returns
//!    for warm_up (finding: it doesn't — workers is a dict, not u32)
//! 3. EmbedResult deserialization handles edge cases

use sidequest_daemon_client::{EmbedResult, StatusResult};

// ===========================================================================
// 1. Error response deserialization — string codes map to -1
// ===========================================================================

#[test]
fn error_payload_deserializes_string_code() {
    // The daemon sends string codes like "EMBED_FAILED". The Rust
    // ErrorPayload must accept these via the flexible deserializer.
    let json = serde_json::json!({
        "id": "00000000-0000-0000-0000-000000000001",
        "error": {
            "code": "EMBED_FAILED",
            "message": "MPS backend out of memory"
        }
    });
    let resp: sidequest_daemon_client::DaemonResponse =
        serde_json::from_value(json).expect("Must deserialize string error code");
    let err = resp.error.expect("Must have error");
    assert_eq!(err.code, -1, "String codes must map to -1");
    assert_eq!(err.message, "MPS backend out of memory");
}

#[test]
fn error_payload_deserializes_integer_code() {
    // Standard JSON-RPC uses integer codes. Must also work.
    let json = serde_json::json!({
        "id": "00000000-0000-0000-0000-000000000002",
        "error": {
            "code": -32600,
            "message": "Invalid Request"
        }
    });
    let resp: sidequest_daemon_client::DaemonResponse =
        serde_json::from_value(json).expect("Must deserialize integer error code");
    let err = resp.error.expect("Must have error");
    assert_eq!(err.code, -32600);
}

#[test]
fn error_payload_with_empty_message_still_deserializes() {
    // An empty error message is valid JSON but produces a confusing
    // display. The Rust side must at least not crash on it.
    let json = serde_json::json!({
        "id": "00000000-0000-0000-0000-000000000003",
        "error": {
            "code": "EMBED_FAILED",
            "message": ""
        }
    });
    let resp: sidequest_daemon_client::DaemonResponse =
        serde_json::from_value(json).expect("Empty message must deserialize");
    let err = resp.error.expect("Must have error");
    assert!(
        err.message.is_empty(),
        "Empty message must deserialize as empty string, not None"
    );
}

// ===========================================================================
// 2. StatusResult vs daemon warm_up response — type mismatch finding
// ===========================================================================

#[test]
fn status_result_deserializes_daemon_warmup_response() {
    // FINDING: The daemon's warm_up handler returns:
    //   {"status": "warm", "workers": {"flux": {...}, "embed": {...}}}
    //
    // But Rust's StatusResult expects:
    //   pub struct StatusResult { pub status: String, pub workers: u32 }
    //
    // This test documents the expected format. If the Rust type doesn't
    // match what the daemon sends, warm_up() calls will fail with
    // DaemonError::InvalidResponse.
    let daemon_response = serde_json::json!({
        "status": "warm",
        "workers": {
            "flux": {"worker": "flux", "status": "warm", "warmup_ms": 5000},
            "embed": {"worker": "embed", "status": "warm", "warmup_ms": 200}
        }
    });
    // This SHOULD succeed if the types match. If it fails, the
    // StatusResult type needs updating.
    let result: Result<StatusResult, _> = serde_json::from_value(daemon_response);
    assert!(
        result.is_ok(),
        "StatusResult must deserialize the daemon's warm_up response. \
         Got error: {:?}. The daemon returns workers as a dict of worker \
         status objects, not a u32 count.",
        result.err()
    );
}

// ===========================================================================
// 3. EmbedResult edge cases
// ===========================================================================

#[test]
fn embed_result_rejects_missing_embedding_field() {
    // If the daemon omits 'embedding', deserialization must fail loudly.
    let json = serde_json::json!({
        "model": "all-MiniLM-L6-v2",
        "latency_ms": 42
    });
    let result: Result<EmbedResult, _> = serde_json::from_value(json);
    assert!(
        result.is_err(),
        "Missing 'embedding' field must fail — no silent defaults"
    );
}

#[test]
fn embed_result_rejects_missing_model_field() {
    // If the daemon omits 'model', deserialization must fail loudly.
    let json = serde_json::json!({
        "embedding": [0.1, 0.2, 0.3],
        "latency_ms": 42
    });
    let result: Result<EmbedResult, _> = serde_json::from_value(json);
    assert!(
        result.is_err(),
        "Missing 'model' field must fail — no silent defaults"
    );
}

#[test]
fn embed_result_rejects_missing_latency_field() {
    // If the daemon omits 'latency_ms', deserialization must fail loudly.
    let json = serde_json::json!({
        "embedding": [0.1, 0.2, 0.3],
        "model": "all-MiniLM-L6-v2"
    });
    let result: Result<EmbedResult, _> = serde_json::from_value(json);
    assert!(
        result.is_err(),
        "Missing 'latency_ms' field must fail — no silent defaults"
    );
}

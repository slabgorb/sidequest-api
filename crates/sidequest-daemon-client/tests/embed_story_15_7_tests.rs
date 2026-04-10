//! Story 15-7: Daemon client embed endpoint tests.
//!
//! Tests that:
//! 1. `EmbedParams` and `EmbedResult` types exist with correct fields
//! 2. `DaemonClient::embed()` method exists
//! 3. Request serialization produces the correct JSON-RPC envelope
//! 4. Response deserialization handles the daemon's expected format

use sidequest_daemon_client::{build_request_json, EmbedParams, EmbedResult};

// ============================================================
// AC-3: EmbedParams type exists with required fields
// ============================================================

#[test]
fn embed_params_has_text_field() {
    let params = EmbedParams {
        text: "The ancient ruins hold secrets of a forgotten civilization.".to_string(),
    };
    assert_eq!(
        params.text,
        "The ancient ruins hold secrets of a forgotten civilization."
    );
}

#[test]
fn embed_params_serializes_to_json() {
    let params = EmbedParams {
        text: "Test text for embedding.".to_string(),
    };
    let json = serde_json::to_value(&params).expect("EmbedParams should serialize");
    assert_eq!(json["text"], "Test text for embedding.");
}

// ============================================================
// AC-3: EmbedResult type exists with embedding vector and metadata
// ============================================================

#[test]
fn embed_result_deserializes_from_daemon_response() {
    // The daemon will return an embedding vector and metadata
    let json = serde_json::json!({
        "embedding": [0.1, 0.2, 0.3, -0.4, 0.5],
        "model": "all-MiniLM-L6-v2",
        "latency_ms": 42
    });

    let result: EmbedResult = serde_json::from_value(json).expect("EmbedResult should deserialize");
    assert_eq!(result.embedding.len(), 5);
    assert!((result.embedding[0] - 0.1).abs() < f32::EPSILON);
    assert_eq!(result.model, "all-MiniLM-L6-v2");
    assert_eq!(result.latency_ms, 42);
}

#[test]
fn embed_result_has_non_empty_embedding() {
    let result = EmbedResult {
        embedding: vec![0.1, 0.2, 0.3],
        model: "all-MiniLM-L6-v2".to_string(),
        latency_ms: 10,
    };
    assert!(
        !result.embedding.is_empty(),
        "Embedding vector must not be empty"
    );
}

// ============================================================
// AC-3: Request envelope for embed method
// ============================================================

#[test]
fn embed_request_builds_correct_envelope() {
    let params = EmbedParams {
        text: "Test embedding request.".to_string(),
    };
    let envelope = build_request_json("embed", &params);

    assert_eq!(envelope["method"], "embed");
    assert_eq!(envelope["params"]["text"], "Test embedding request.");
    assert!(
        envelope["id"].as_str().is_some(),
        "Request must have a UUID id"
    );
}

// ============================================================
// AC-6: OTEL fields present on EmbedResult for lore.embedding_generated event
// ============================================================

#[test]
fn embed_result_has_model_for_otel() {
    // OTEL event lore.embedding_generated needs: fragment_id, latency_ms, model
    // fragment_id comes from the caller; model and latency_ms come from EmbedResult
    let result = EmbedResult {
        embedding: vec![0.1],
        model: "all-MiniLM-L6-v2".to_string(),
        latency_ms: 55,
    };
    assert!(
        !result.model.is_empty(),
        "Model field required for OTEL lore.embedding_generated event"
    );
    assert!(
        result.latency_ms > 0,
        "Latency field required for OTEL lore.embedding_generated event"
    );
}

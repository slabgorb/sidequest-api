use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Request envelope
// ---------------------------------------------------------------------------

/// JSON-RPC style request envelope sent to the daemon.
#[derive(Debug, Clone, Serialize)]
pub struct DaemonRequest<P: Serialize> {
    pub id: Uuid,
    pub method: String,
    pub params: P,
}

// ---------------------------------------------------------------------------
// Method-specific parameter types (stubs — Dev fills in fields)
// ---------------------------------------------------------------------------

/// Parameters for a `render` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderParams {
    /// The image generation prompt.
    pub prompt: String,
    /// Art style to apply (e.g. "oil_painting", "pixel_art").
    pub art_style: String,
}

/// Parameters for a `warm_up` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WarmUpParams {
    /// The worker/model to warm up (e.g. "flux", "kokoro").
    pub worker: String,
}

// ---------------------------------------------------------------------------
// Response envelope
// ---------------------------------------------------------------------------

/// JSON-RPC style response envelope from the daemon.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DaemonResponse {
    pub id: Uuid,
    pub result: Option<serde_json::Value>,
    pub error: Option<ErrorPayload>,
}

/// Error payload returned by the daemon inside a response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErrorPayload {
    pub code: i32,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Method-specific result types (stubs — Dev fills in fields)
// ---------------------------------------------------------------------------

/// Result from a `render` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderResult {
    /// Path to the generated image.
    pub image_url: String,
    /// Time taken to generate the image in milliseconds.
    pub generation_ms: u64,
}

/// Result from a `warm_up` / `status` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusResult {
    /// Current daemon status (e.g. "ready", "warming_up").
    pub status: String,
    /// Number of active workers.
    pub workers: u32,
}

// ---------------------------------------------------------------------------
// Request builder (stub — Dev implements)
// ---------------------------------------------------------------------------

/// Build the JSON-RPC request envelope for a given method and params.
///
/// Returns a `serde_json::Value` with `id` (UUID v4), `method`, and `params` fields.
pub fn build_request_json(method: &str, params: &impl Serialize) -> serde_json::Value {
    serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "method": method,
        "params": serde_json::to_value(params).unwrap_or(serde_json::Value::Object(Default::default())),
    })
}

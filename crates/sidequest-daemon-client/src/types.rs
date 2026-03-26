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
pub struct RenderParams {}

/// Parameters for a `warm_up` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WarmUpParams {}

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
pub struct RenderResult {}

/// Result from a `warm_up` / `status` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusResult {}

// ---------------------------------------------------------------------------
// Request builder (stub — Dev implements)
// ---------------------------------------------------------------------------

/// Build the JSON-RPC request envelope for a given method and params.
///
/// Returns a `serde_json::Value` with `id` (UUID v4), `method`, and `params` fields.
pub fn build_request_json(_method: &str, _params: &impl Serialize) -> serde_json::Value {
    todo!("build_request_json: Dev implements request envelope construction")
}

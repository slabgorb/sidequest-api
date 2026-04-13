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
// Method-specific parameter types
// ---------------------------------------------------------------------------

/// Parameters for a `render` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderParams {
    /// The image generation prompt (raw subject fragment).
    pub prompt: String,
    /// Art style to apply (e.g. "oil_painting", "pixel_art").
    pub art_style: String,
    /// Render tier — routes to the correct daemon worker.
    /// One of: "scene_illustration", "portrait", "landscape", "cartography".
    pub tier: String,
    /// Pre-composed positive prompt with genre style suffix and tag overrides baked in.
    /// When set, the daemon's flux worker uses this directly instead of building from parts.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub positive_prompt: String,
    /// Negative prompt from the genre's visual_style.yaml.
    /// Flux doesn't use negative prompts natively, but future models (SDXL) will.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub negative_prompt: String,
    /// Raw narration text — when present, the daemon runs LLM-based SubjectExtractor
    /// to produce visual image prompts instead of using the raw `prompt` field.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub narration: String,
    /// Target image width in pixels (from tier_to_dimensions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Target image height in pixels (from tier_to_dimensions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Flux variant override: `"dev"` or `"schnell"`. Empty string means
    /// "use the daemon's tier default" (see flux_mlx_worker.py
    /// `TIER_CONFIGS[tier]["model"]`). Sourced from the genre pack's
    /// `visual_style.yaml::preferred_model`. Previously the YAML field
    /// was read by Rust and silently dropped at the enqueue boundary;
    /// story 35-15 closes that wire. The daemon validates: non-empty
    /// values must be in `{"dev", "schnell"}` — unknown variants raise
    /// loudly (no silent fallback).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub variant: String,
    /// Optional absolute path to a `.safetensors` LoRA file. When set,
    /// the daemon's FluxMLXWorker constructs Flux1 with `lora_paths=[path]`
    /// instead of using the base model. Read at
    /// `sidequest_daemon/media/workers/flux_mlx_worker.py:155`. Per
    /// ADR-032. Story 35-15.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lora_path: Option<String>,
    /// Optional LoRA scale (typically 0.0–1.0). When absent, the daemon
    /// defaults to 1.0 (full weight) at
    /// `sidequest_daemon/media/workers/flux_mlx_worker.py:156`. The Rust
    /// side sends `None` rather than silently defaulting to 1.0 — the
    /// daemon owns the default (no silent fallback). Story 35-15.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lora_scale: Option<f32>,
}

/// Parameters for a `warm_up` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WarmUpParams {
    /// The worker/model to warm up (e.g. "flux", "kokoro").
    pub worker: String,
}

/// Parameters for an `embed` request (story 15-7).
///
/// Sent to the daemon's embed worker to generate sentence embeddings
/// for semantic lore retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedParams {
    /// The text to generate an embedding for.
    pub text: String,
}

/// Result from an `embed` request (story 15-7).
///
/// Contains the embedding vector and metadata for OTEL telemetry
/// (`lore.embedding_generated` event needs `model` and `latency_ms`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedResult {
    /// The embedding vector (e.g. 384-dimensional for all-MiniLM-L6-v2).
    pub embedding: Vec<f32>,
    /// Name of the embedding model used.
    pub model: String,
    /// Time taken to generate the embedding in milliseconds.
    pub latency_ms: u64,
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
/// The daemon sends string codes (e.g., "PARSE_ERROR") while JSON-RPC spec uses ints.
/// We accept both via a flexible deserializer.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErrorPayload {
    #[serde(deserialize_with = "deserialize_error_code")]
    pub code: i32,
    pub message: String,
}

/// Accept both integer and string error codes from the daemon.
/// String codes are mapped to -1 (unknown) since the daemon uses string codes
/// like "PARSE_ERROR", "INVALID_REQUEST", etc.
fn deserialize_error_code<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    struct ErrorCodeVisitor;
    impl<'de> de::Visitor<'de> for ErrorCodeVisitor {
        type Value = i32;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an integer or string error code")
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<i32, E> {
            Ok(v as i32)
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<i32, E> {
            Ok(v as i32)
        }
        fn visit_str<E: de::Error>(self, _v: &str) -> Result<i32, E> {
            Ok(-1) // String error codes map to -1
        }
    }
    deserializer.deserialize_any(ErrorCodeVisitor)
}

// ---------------------------------------------------------------------------
// Method-specific result types
// ---------------------------------------------------------------------------

/// Result from a `render` request.
///
/// The daemon returns `image_path` as the field name, but we normalize to
/// `image_url` on our side. The `alias` attributes let serde accept either name.
///
/// NOTE: `image_url` intentionally has NO `#[serde(default)]`. If the daemon
/// omits the image path entirely, deserialization will FAIL — and that's what
/// we want. A missing image path is a bug, not a graceful degradation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    /// Path to the generated image.
    /// Accepts `image_url`, `image_path`, `output_path`, `path`, or `file` from the daemon.
    /// No default — if the daemon doesn't return a path, we want a loud error.
    #[serde(
        alias = "image_path",
        alias = "output_path",
        alias = "path",
        alias = "file"
    )]
    pub image_url: String,
    /// Time taken to generate the image in milliseconds.
    /// Accepts both `generation_ms` and `elapsed_ms` from the daemon.
    #[serde(default, alias = "elapsed_ms")]
    pub generation_ms: u64,
}

/// Result from a `warm_up` / `status` request.
///
/// The daemon's warm_up handler returns `workers` as a dict of per-worker
/// status objects (e.g. `{"flux": {"status": "warm", ...}, "embed": {...}}`),
/// not a count. We accept `serde_json::Value` to handle both the warm_up
/// response (dict) and the status response (may vary). Story 37-5.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusResult {
    /// Current daemon status (e.g. "ready", "warm").
    pub status: String,
    /// Worker details — dict of per-worker status objects from warm_up,
    /// or other shapes from status. Accepts any JSON value.
    #[serde(default)]
    pub workers: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Request builder
// ---------------------------------------------------------------------------

/// Build the JSON-RPC request envelope for a given method and params.
///
/// Returns a `serde_json::Value` with `id` (UUID v4), `method`, and `params` fields.
pub fn build_request_json(method: &str, params: &impl Serialize) -> serde_json::Value {
    serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "method": method,
        "params": serde_json::to_value(params).expect("Failed to serialize RPC params"),
    })
}

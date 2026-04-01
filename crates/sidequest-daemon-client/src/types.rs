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
    /// One of: "scene_illustration", "portrait", "landscape", "cartography", "tts", "music".
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
}

/// Parameters for a `tts` (text-to-speech) request.
///
/// Sent via the `render` method — the daemon dispatches by `tier` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsParams {
    /// The text to synthesize.
    pub text: String,
    /// TTS model name (e.g. "kokoro", "piper").
    pub model: String,
    /// Voice ID within the model.
    pub voice_id: String,
    /// Speech speed multiplier (1.0 = normal).
    pub speed: f32,
    /// Render tier — tells the daemon to route to the TTS worker.
    #[serde(default = "default_tts_tier")]
    pub tier: String,
}

fn default_tts_tier() -> String {
    "tts".to_string()
}

impl Default for TtsParams {
    fn default() -> Self {
        Self {
            text: String::new(),
            model: String::new(),
            voice_id: String::new(),
            speed: 1.0,
            tier: default_tts_tier(),
        }
    }
}

/// Result from a `tts` request.
///
/// The daemon returns `audio_bytes` (raw PCM s16le as a JSON array of ints)
/// and optionally `audio_path` (file on disk). All fields use `serde(default)`
/// so deserialization succeeds even if the daemon omits a field.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TtsResult {
    /// Raw audio bytes (PCM s16le at 24 kHz).
    #[serde(default)]
    pub audio_bytes: Vec<u8>,
    /// Duration of the audio in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,
    /// Wall-clock synthesis time in milliseconds.
    #[serde(default, alias = "generation_ms")]
    pub elapsed_ms: u64,
    /// Voice preset name used for synthesis.
    #[serde(default)]
    pub voice: String,
    /// Path to the WAV file on the daemon host (fallback if audio_bytes empty).
    #[serde(default)]
    pub audio_path: String,
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
    #[serde(alias = "image_path", alias = "output_path", alias = "path", alias = "file")]
    pub image_url: String,
    /// Time taken to generate the image in milliseconds.
    /// Accepts both `generation_ms` and `elapsed_ms` from the daemon.
    #[serde(default, alias = "elapsed_ms")]
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

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
    /// Render tier — routes to the correct daemon worker.
    /// One of: "scene_illustration", "portrait", "landscape", "cartography", "tts", "music".
    pub tier: String,
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
    /// Accepts `image_url`, `output_path`, `path`, or `file` from the daemon.
    #[serde(default, alias = "output_path", alias = "path", alias = "file")]
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

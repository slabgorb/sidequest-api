use std::path::PathBuf;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::error::DaemonError;
use crate::types::{
    build_request_json, DaemonResponse, RenderParams, RenderResult, StatusResult, TtsParams,
    TtsResult, WarmUpParams,
};

/// Configuration for connecting to the daemon.
pub struct DaemonConfig {
    pub socket_path: PathBuf,
    pub render_timeout: Duration,
    pub default_timeout: Duration,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/tmp/sidequest-renderer.sock"),
            render_timeout: Duration::from_secs(300),
            default_timeout: Duration::from_secs(10),
        }
    }
}

/// Async client for communicating with sidequest-daemon over Unix socket.
pub struct DaemonClient {
    stream: BufReader<UnixStream>,
    config: DaemonConfig,
}

impl DaemonClient {
    /// Connect to the daemon at the configured socket path.
    pub async fn connect(config: DaemonConfig) -> Result<Self, DaemonError> {
        let stream = UnixStream::connect(&config.socket_path).await?;
        Ok(Self {
            stream: BufReader::new(stream),
            config,
        })
    }

    /// Send a JSON-RPC request and read the response.
    async fn request(
        &mut self,
        method: &str,
        params: &impl serde::Serialize,
        timeout: Duration,
    ) -> Result<DaemonResponse, DaemonError> {
        let req = build_request_json(method, params);
        let mut line =
            serde_json::to_string(&req).map_err(|e| DaemonError::InvalidResponse(e.to_string()))?;
        line.push('\n');

        let result = tokio::time::timeout(timeout, async {
            self.stream.get_mut().write_all(line.as_bytes()).await?;
            self.stream.get_mut().flush().await?;
            let mut response_line = String::new();
            self.stream.read_line(&mut response_line).await?;
            Ok::<_, std::io::Error>(response_line)
        })
        .await
        .map_err(|_| DaemonError::Timeout { duration: timeout })?
        .map_err(DaemonError::SocketError)?;

        let resp: DaemonResponse = serde_json::from_str(&result)
            .map_err(|e| DaemonError::InvalidResponse(format!("{e}: {result}")))?;

        if let Some(err) = resp.error {
            return Err(DaemonError::DaemonErrorResponse {
                code: err.code,
                message: err.message,
            });
        }

        Ok(resp)
    }

    /// Health check — returns Ok if daemon is responsive.
    pub async fn ping(&mut self) -> Result<(), DaemonError> {
        self.request("ping", &serde_json::json!({}), self.config.default_timeout)
            .await?;
        Ok(())
    }

    /// Send a render request and wait for the result.
    ///
    /// Logs OTel-style events for the full render lifecycle:
    /// - render_requested: what we're asking the daemon for
    /// - render_result_received: success with URL and timing
    /// - render_deserialize_failed: LOUD error if JSON doesn't match RenderResult
    /// - render_empty_url: LOUD error if daemon returns empty/blank image path
    pub async fn render(&mut self, params: RenderParams) -> Result<RenderResult, DaemonError> {
        let span = tracing::info_span!(
            "daemon.render",
            tier = %params.tier,
            art_style = %params.art_style,
            prompt_len = params.prompt.len(),
            duration_ms = tracing::field::Empty,
        );
        let _guard = span.enter();

        tracing::info!("render_requested");

        let resp = self
            .request("render", &params, self.config.render_timeout)
            .await?;

        let raw_result = resp
            .result
            .ok_or_else(|| DaemonError::InvalidResponse("missing result".into()))?;

        // Log the raw JSON so we can debug field-name mismatches in the watch log.
        tracing::debug!(raw_json = %raw_result, "render_raw_response");

        // Deserialize — NO silent defaults. If image_url/image_path is missing,
        // serde will fail here and we catch it loudly.
        let render_result: RenderResult = serde_json::from_value(raw_result.clone())
            .map_err(|e| {
                // This is the "scream in the watch log" Keith asked for.
                // If we hit this, the daemon returned JSON that doesn't have any
                // recognized image path field (image_url, image_path, output_path, etc.)
                tracing::error!(
                    error = %e,
                    raw_json = %raw_result,
                    tier = %params.tier,
                    "render_deserialize_failed — daemon response missing image path field"
                );
                DaemonError::InvalidResponse(e.to_string())
            })?;

        // Belt AND suspenders: even if serde accepted the field, reject empty strings.
        // An empty URL means a broken <img> tag downstream.
        if render_result.image_url.trim().is_empty() {
            tracing::error!(
                raw_json = %raw_result,
                tier = %params.tier,
                "render_empty_url — daemon returned blank image path, this will produce a broken image"
            );
            return Err(DaemonError::InvalidResponse(
                "daemon returned empty image_url/image_path".into(),
            ));
        }

        span.record("duration_ms", render_result.generation_ms);
        tracing::info!(
            image_url = %render_result.image_url,
            generation_ms = render_result.generation_ms,
            "render_result_received"
        );

        Ok(render_result)
    }

    /// Send a warm_up request to pre-load a worker model.
    pub async fn warm_up(&mut self, params: WarmUpParams) -> Result<StatusResult, DaemonError> {
        let resp = self
            .request("warm_up", &params, self.config.default_timeout)
            .await?;
        let result = resp
            .result
            .ok_or_else(|| DaemonError::InvalidResponse("missing result".into()))?;
        serde_json::from_value(result).map_err(|e| DaemonError::InvalidResponse(e.to_string()))
    }

    /// Synthesize text to speech audio bytes.
    pub async fn synthesize(&mut self, params: TtsParams) -> Result<TtsResult, DaemonError> {
        let span = tracing::info_span!(
            "daemon.synthesize",
            text_len = params.text.len(),
            voice_id = %params.voice_id,
            duration_ms = tracing::field::Empty,
        );
        let _guard = span.enter();

        let resp = self
            .request("render", &params, self.config.render_timeout)
            .await?;
        let result = resp
            .result
            .ok_or_else(|| DaemonError::InvalidResponse("missing result".into()))?;
        let tts_result: TtsResult = serde_json::from_value(result)
            .map_err(|e| DaemonError::InvalidResponse(e.to_string()))?;
        span.record("duration_ms", tts_result.elapsed_ms);
        Ok(tts_result)
    }

    /// Generate a sentence embedding for the given text (story 15-7).
    ///
    /// Calls the daemon's `embed` method, which runs a sentence-transformer
    /// model and returns the embedding vector with timing metadata.
    pub async fn embed(
        &mut self,
        params: crate::types::EmbedParams,
    ) -> Result<crate::types::EmbedResult, DaemonError> {
        let span = tracing::info_span!(
            "daemon.embed",
            text_len = params.text.len(),
            latency_ms = tracing::field::Empty,
        );
        let _guard = span.enter();

        let resp = self
            .request("embed", &params, self.config.default_timeout)
            .await?;
        let result = resp
            .result
            .ok_or_else(|| DaemonError::InvalidResponse("missing result".into()))?;
        let embed_result: crate::types::EmbedResult = serde_json::from_value(result)
            .map_err(|e| DaemonError::InvalidResponse(e.to_string()))?;
        span.record("latency_ms", embed_result.latency_ms);
        Ok(embed_result)
    }

    /// Request a graceful daemon shutdown.
    pub async fn shutdown(&mut self) -> Result<(), DaemonError> {
        self.request(
            "shutdown",
            &serde_json::json!({}),
            self.config.default_timeout,
        )
        .await?;
        Ok(())
    }
}

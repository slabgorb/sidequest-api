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
    pub async fn render(&mut self, params: RenderParams) -> Result<RenderResult, DaemonError> {
        let resp = self
            .request("render", &params, self.config.render_timeout)
            .await?;
        let result = resp
            .result
            .ok_or_else(|| DaemonError::InvalidResponse("missing result".into()))?;
        serde_json::from_value(result).map_err(|e| DaemonError::InvalidResponse(e.to_string()))
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
        let resp = self
            .request("render", &params, self.config.render_timeout)
            .await?;
        let result = resp
            .result
            .ok_or_else(|| DaemonError::InvalidResponse("missing result".into()))?;
        serde_json::from_value(result).map_err(|e| DaemonError::InvalidResponse(e.to_string()))
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

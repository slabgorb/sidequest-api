//! Claude CLI subprocess client.
//!
//! Port lesson #3: Single ClaudeClient with configurable timeout,
//! consistent error types, and a standard fallback policy.

use std::time::Duration;
use tracing::{error, warn};

/// Default timeout for Claude CLI invocations (120 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Default command path for Claude CLI.
const DEFAULT_COMMAND: &str = "claude";

/// Errors from Claude CLI subprocess invocations.
#[derive(Debug)]
#[non_exhaustive]
pub enum ClaudeClientError {
    /// The subprocess exceeded the configured timeout.
    Timeout {
        /// How long we waited before giving up.
        elapsed: Duration,
    },
    /// The subprocess exited with a non-zero status.
    SubprocessFailed {
        /// Exit code, if available.
        exit_code: Option<i32>,
        /// Captured stderr output.
        stderr: String,
    },
    /// The subprocess returned an empty response.
    EmptyResponse,
}

impl std::fmt::Display for ClaudeClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaudeClientError::Timeout { elapsed } => {
                write!(f, "Claude CLI timed out after {elapsed:?}")
            }
            ClaudeClientError::SubprocessFailed { exit_code, stderr } => {
                write!(f, "Claude CLI failed (exit code: {exit_code:?}): {stderr}")
            }
            ClaudeClientError::EmptyResponse => {
                write!(f, "Claude CLI returned an empty response")
            }
        }
    }
}

impl std::error::Error for ClaudeClientError {}

/// Response from a Claude CLI invocation, including token usage telemetry.
#[derive(Debug, Clone)]
pub struct ClaudeResponse {
    /// The text content of the response.
    pub text: String,
    /// Input tokens consumed (from `--output-format json` envelope).
    pub input_tokens: Option<u64>,
    /// Output tokens produced (from `--output-format json` envelope).
    pub output_tokens: Option<u64>,
}

/// Claude CLI subprocess client with configurable timeout and command path.
#[derive(Debug, Clone)]
pub struct ClaudeClient {
    timeout: Duration,
    command_path: String,
}

impl ClaudeClient {
    /// Create a new client with default settings (120s timeout, "claude" command).
    pub fn new() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            command_path: DEFAULT_COMMAND.to_string(),
        }
    }

    /// Create a new client with a custom timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout,
            command_path: DEFAULT_COMMAND.to_string(),
        }
    }

    /// Create a builder for more complex configuration.
    pub fn builder() -> ClaudeClientBuilder {
        ClaudeClientBuilder::default()
    }

    /// The configured timeout duration.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// The configured command path.
    pub fn command_path(&self) -> &str {
        &self.command_path
    }
}

impl ClaudeClient {
    /// Execute a synchronous subprocess call with a specific model.
    ///
    /// Passes `--model <model>` before `-p <prompt>`. Returns stdout on success.
    pub fn send_with_model(
        &self,
        prompt: &str,
        model: &str,
    ) -> Result<ClaudeResponse, ClaudeClientError> {
        self.send_impl(prompt, Some(model))
    }

    /// Core subprocess execution — used by both `send` and `send_with_model`.
    ///
    /// Calls `claude -p` with `--output-format json` to capture token usage
    /// and cost alongside the text response. Token counts are recorded on the
    /// tracing span for OTEL consumption.
    fn send_impl(
        &self,
        prompt: &str,
        model: Option<&str>,
    ) -> Result<ClaudeResponse, ClaudeClientError> {
        use std::io::Read;
        use std::process::{Command, Stdio};
        use std::time::Instant;

        let model_label = model.unwrap_or("default");
        let span = tracing::info_span!(
            "agent.call",
            model = %model_label,
            prompt_len = prompt.len(),
            response_len = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
            input_tokens = tracing::field::Empty,
            output_tokens = tracing::field::Empty,
            cost_usd = tracing::field::Empty,
        );
        let _guard = span.enter();

        if prompt.trim().is_empty() {
            return Err(ClaudeClientError::EmptyResponse);
        }

        let mut cmd = Command::new(&self.command_path);
        if let Some(m) = model {
            cmd.arg("--model").arg(m);
        }
        cmd.arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("json")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            tracing::error!(command = %self.command_path, model = %model_label, error = %e, "Failed to spawn subprocess");
            ClaudeClientError::SubprocessFailed {
                exit_code: None,
                stderr: e.to_string(),
            }
        })?;

        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let mut stdout = String::new();
                    let mut stderr = String::new();
                    if let Some(mut out) = child.stdout.take() {
                        out.read_to_string(&mut stdout).map_err(|e| {
                            ClaudeClientError::SubprocessFailed {
                                exit_code: status.code(),
                                stderr: format!("stdout read error: {e}"),
                            }
                        })?;
                    }
                    if let Some(mut err) = child.stderr.take() {
                        err.read_to_string(&mut stderr).map_err(|e| {
                            ClaudeClientError::SubprocessFailed {
                                exit_code: status.code(),
                                stderr: format!("stderr read error: {e}"),
                            }
                        })?;
                    }

                    if !status.success() {
                        return Err(ClaudeClientError::SubprocessFailed {
                            exit_code: status.code(),
                            stderr,
                        });
                    }

                    let trimmed = stdout.trim().to_string();
                    if trimmed.is_empty() {
                        return Err(ClaudeClientError::EmptyResponse);
                    }

                    // Parse JSON envelope from --output-format json
                    let mut input_tokens: Option<u64> = None;
                    let mut output_tokens: Option<u64> = None;
                    let text = if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(&trimmed) {
                        // Extract token counts from usage block
                        if let Some(usage) = envelope.get("usage") {
                            if let Some(inp) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
                                span.record("input_tokens", inp);
                                input_tokens = Some(inp);
                            }
                            if let Some(out) = usage.get("output_tokens").and_then(|v| v.as_u64()) {
                                span.record("output_tokens", out);
                                output_tokens = Some(out);
                            }
                        }
                        if let Some(cost) = envelope.get("total_cost_usd").and_then(|v| v.as_f64()) {
                            span.record("cost_usd", cost);
                        }
                        // Extract the actual text result
                        envelope.get("result")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&trimmed)
                            .to_string()
                    } else {
                        // Fallback: not JSON (shouldn't happen with --output-format json)
                        trimmed
                    };

                    if text.is_empty() {
                        return Err(ClaudeClientError::EmptyResponse);
                    }
                    span.record("response_len", text.len());
                    span.record("duration_ms", start.elapsed().as_millis() as u64);
                    return Ok(ClaudeResponse { text, input_tokens, output_tokens });
                }
                Ok(None) => {
                    if start.elapsed() > self.timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        let elapsed = start.elapsed();
                        span.record("duration_ms", start.elapsed().as_millis() as u64);
                        tracing::warn!(timeout = ?self.timeout, ?elapsed, model = %model_label, "Claude CLI subprocess timed out");
                        return Err(ClaudeClientError::Timeout { elapsed });
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to check subprocess status");
                    return Err(ClaudeClientError::SubprocessFailed {
                        exit_code: None,
                        stderr: e.to_string(),
                    });
                }
            }
        }
    }

    /// Execute a synchronous subprocess call with the configured command and timeout.
    pub fn send(&self, prompt: &str) -> Result<ClaudeResponse, ClaudeClientError> {
        self.send_impl(prompt, None)
    }
}

impl Default for ClaudeClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a JSON envelope `{"result": "text"}` and extract the inner text.
///
/// Returns `None` if the input is not valid JSON or doesn't contain a "result" field.
pub fn parse_json_envelope(input: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(input).ok()?;
    value.get("result")?.as_str().map(|s| s.to_string())
}

/// Builder for ClaudeClient configuration.
#[derive(Debug)]
pub struct ClaudeClientBuilder {
    timeout: Duration,
    command_path: String,
}

impl Default for ClaudeClientBuilder {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            command_path: DEFAULT_COMMAND.to_string(),
        }
    }
}

impl ClaudeClientBuilder {
    /// Set the timeout duration.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the command path.
    pub fn command_path(mut self, path: impl Into<String>) -> Self {
        self.command_path = path.into();
        self
    }

    /// Build the ClaudeClient.
    pub fn build(self) -> ClaudeClient {
        ClaudeClient {
            timeout: self.timeout,
            command_path: self.command_path,
        }
    }
}

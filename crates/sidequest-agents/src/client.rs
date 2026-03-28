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

/// Default max tokens for narration responses.
const DEFAULT_MAX_TOKENS: u32 = 1024;

/// Claude CLI subprocess client with configurable timeout and command path.
#[derive(Debug, Clone)]
pub struct ClaudeClient {
    timeout: Duration,
    command_path: String,
    max_tokens: Option<u32>,
}

impl ClaudeClient {
    /// Create a new client with default settings (120s timeout, "claude" command, 1024 max tokens).
    pub fn new() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            command_path: DEFAULT_COMMAND.to_string(),
            max_tokens: Some(DEFAULT_MAX_TOKENS),
        }
    }

    /// Create a new client with a custom timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout,
            command_path: DEFAULT_COMMAND.to_string(),
            max_tokens: Some(DEFAULT_MAX_TOKENS),
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
    ) -> Result<String, ClaudeClientError> {
        use std::io::Read;
        use std::process::{Command, Stdio};
        use std::time::Instant;

        if prompt.trim().is_empty() {
            return Err(ClaudeClientError::EmptyResponse);
        }

        let mut cmd = Command::new(&self.command_path);
        cmd.arg("--model").arg(model);
        if let Some(max) = self.max_tokens {
            cmd.arg("--max-tokens").arg(max.to_string());
        }
        cmd.arg("-p").arg(prompt);
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                tracing::error!(command = %self.command_path, model = %model, error = %e, "Failed to spawn subprocess");
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
                    return Ok(trimmed);
                }
                Ok(None) => {
                    if start.elapsed() > self.timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        let elapsed = start.elapsed();
                        tracing::warn!(timeout = ?self.timeout, ?elapsed, model = %model, "Claude CLI subprocess timed out");
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
    ///
    /// Passes the prompt as a single argument. Returns stdout on success.
    pub fn send(&self, prompt: &str) -> Result<String, ClaudeClientError> {
        use std::io::Read;
        use std::process::{Command, Stdio};
        use std::time::Instant;

        if prompt.trim().is_empty() {
            return Err(ClaudeClientError::EmptyResponse);
        }

        let mut cmd = Command::new(&self.command_path);
        if let Some(max) = self.max_tokens {
            cmd.arg("--max-tokens").arg(max.to_string());
        }
        cmd.arg("-p").arg(prompt);
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                error!(command = %self.command_path, error = %e, "Failed to spawn subprocess");
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
                    return Ok(trimmed);
                }
                Ok(None) => {
                    if start.elapsed() > self.timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        let elapsed = start.elapsed();
                        warn!(timeout = ?self.timeout, ?elapsed, "Claude CLI subprocess timed out");
                        return Err(ClaudeClientError::Timeout { elapsed });
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    error!(error = %e, "Failed to check subprocess status");
                    return Err(ClaudeClientError::SubprocessFailed {
                        exit_code: None,
                        stderr: e.to_string(),
                    });
                }
            }
        }
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
    max_tokens: Option<u32>,
}

impl Default for ClaudeClientBuilder {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            command_path: DEFAULT_COMMAND.to_string(),
            max_tokens: Some(DEFAULT_MAX_TOKENS),
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

    /// Set the max tokens for responses (None = unlimited).
    pub fn max_tokens(mut self, max: Option<u32>) -> Self {
        self.max_tokens = max;
        self
    }

    /// Build the ClaudeClient.
    pub fn build(self) -> ClaudeClient {
        ClaudeClient {
            timeout: self.timeout,
            command_path: self.command_path,
            max_tokens: self.max_tokens,
        }
    }
}

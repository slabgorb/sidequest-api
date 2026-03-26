use std::time::Duration;

/// Errors that can occur when communicating with the daemon.
#[derive(Debug)]
#[non_exhaustive]
pub enum DaemonError {
    /// Failed to connect or communicate over the Unix socket.
    SocketError(std::io::Error),
    /// Request exceeded the configured timeout.
    Timeout { duration: Duration },
    /// Response could not be parsed as valid JSON or was missing expected fields.
    InvalidResponse(String),
    /// Daemon returned an explicit error response.
    DaemonErrorResponse { code: i32, message: String },
}

// Placeholder Display — Dev replaces with thiserror derive or proper messages.
impl std::fmt::Display for DaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "daemon error")
    }
}

impl std::error::Error for DaemonError {}

impl From<std::io::Error> for DaemonError {
    fn from(err: std::io::Error) -> Self {
        DaemonError::SocketError(err)
    }
}

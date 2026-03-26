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

impl std::fmt::Display for DaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonError::SocketError(err) => write!(f, "socket error: {err}"),
            DaemonError::Timeout { duration } => {
                write!(f, "request timeout after {}s", duration.as_secs())
            }
            DaemonError::InvalidResponse(detail) => {
                write!(f, "invalid response: {detail}")
            }
            DaemonError::DaemonErrorResponse { code, message } => {
                write!(f, "daemon error ({code}): {message}")
            }
        }
    }
}

impl std::error::Error for DaemonError {}

impl From<std::io::Error> for DaemonError {
    fn from(err: std::io::Error) -> Self {
        DaemonError::SocketError(err)
    }
}

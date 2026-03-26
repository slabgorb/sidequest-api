use std::path::PathBuf;
use std::time::Duration;

use crate::error::DaemonError;
use crate::types::{RenderParams, RenderResult, StatusResult, WarmUpParams};

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
            render_timeout: Duration::from_secs(30),
            default_timeout: Duration::from_secs(10),
        }
    }
}

/// Async client for communicating with sidequest-daemon over Unix socket.
pub struct DaemonClient {
    _private: (),
}

impl DaemonClient {
    /// Connect to the daemon at the configured socket path.
    pub async fn connect(_config: DaemonConfig) -> Result<Self, DaemonError> {
        todo!("DaemonClient::connect")
    }

    /// Health check — returns Ok if daemon is responsive.
    pub async fn ping(&self) -> Result<(), DaemonError> {
        todo!("DaemonClient::ping")
    }

    /// Send a render request and wait for the result.
    pub async fn render(&self, _params: RenderParams) -> Result<RenderResult, DaemonError> {
        todo!("DaemonClient::render")
    }

    /// Send a warm_up request to pre-load a worker model.
    pub async fn warm_up(&self, _params: WarmUpParams) -> Result<StatusResult, DaemonError> {
        todo!("DaemonClient::warm_up")
    }

    /// Request a graceful daemon shutdown.
    pub async fn shutdown(&self) -> Result<(), DaemonError> {
        todo!("DaemonClient::shutdown")
    }
}

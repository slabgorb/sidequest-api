//! Client for communicating with sidequest-daemon over Unix socket.
//!
//! The daemon speaks a JSON-RPC style protocol over a Unix domain socket.
//! Each request is a newline-delimited JSON object with `id`, `method`,
//! and `params` fields. Responses contain `id` plus either `result` or `error`.

mod client;
mod error;
mod types;

pub use client::{DaemonClient, DaemonConfig};
pub use error::DaemonError;
pub use types::*;

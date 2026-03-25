//! SideQuest Protocol — GameMessage enum and typed payloads.
//!
//! This crate defines the communication protocol between the UI and the game server,
//! including all game messages and their JSON serialization.

#![warn(missing_docs)]

/// Protocol version for compatibility checking.
pub const PROTOCOL_VERSION: &str = "0.1.0";

// These modules will be created by Dev (Mal) during the green phase.
// For now, tests reference types that don't exist yet — that's the point of RED.

#[cfg(test)]
mod tests;

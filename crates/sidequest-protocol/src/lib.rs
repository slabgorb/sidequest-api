//! SideQuest Protocol — GameMessage enum and typed payloads.
//!
//! This crate defines the communication protocol between the UI and the game server,
//! including all game messages, validated newtypes, and input sanitization.

#![warn(missing_docs)]

pub mod message;
pub mod sanitize;
pub mod types;

// Re-export core types at crate root for convenience.
pub use message::*;
pub use sanitize::sanitize_player_text;
pub use types::*;

/// Protocol version for compatibility checking.
pub const PROTOCOL_VERSION: &str = "0.1.0";

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "action_reveal_tests.rs"]
mod action_reveal_tests;

#[cfg(test)]
#[path = "narrator_verbosity_story_14_3_tests.rs"]
mod narrator_verbosity_story_14_3_tests;

#[cfg(test)]
#[path = "narrator_vocabulary_story_14_4_tests.rs"]
mod narrator_vocabulary_story_14_4_tests;

#[cfg(test)]
#[path = "journal_story_9_13_tests.rs"]
mod journal_story_9_13_tests;

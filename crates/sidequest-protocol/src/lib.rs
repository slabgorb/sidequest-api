//! SideQuest Protocol — GameMessage enum and typed payloads.
//!
//! This crate defines the communication protocol between the UI and the game server,
//! including all game messages and their JSON serialization.

#![warn(missing_docs)]

/// Protocol version for compatibility checking.
pub const PROTOCOL_VERSION: &str = "0.1.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(PROTOCOL_VERSION, "0.1.0");
    }
}

//! Per-connection session state machine.
//!
//! Each WebSocket connection owns a Session that tracks the player's progress:
//! `AwaitingConnect` → `Creating` → `Playing`.
//!
//! Messages are dispatched based on current state — out-of-phase messages
//! are rejected with an error, not a crash.

use sidequest_protocol::GameMessage;

use crate::ServerError;

/// Per-connection session state machine.
pub struct Session {
    state: SessionState,
}

enum SessionState {
    AwaitingConnect,
    Creating {
        genre_slug: String,
        world_slug: String,
        player_name: String,
    },
    Playing {
        genre_slug: String,
        world_slug: String,
        player_name: String,
    },
}

impl Session {
    /// Create a new session in the AwaitingConnect state.
    pub fn new() -> Self {
        Self {
            state: SessionState::AwaitingConnect,
        }
    }

    /// Handle a SESSION_EVENT{connect} — bind genre/world and transition to Creating.
    /// Returns a SESSION_EVENT{connected} response message.
    pub fn handle_connect(
        &mut self,
        genre: &str,
        world: &str,
        player_name: &str,
    ) -> Result<GameMessage, ServerError> {
        // Accept connect from ANY state — the client may be reconnecting after
        // a server restart, browser refresh, or network drop.  Rejecting with
        // "already connected" leaves the client dead with no recovery path.
        if !self.is_awaiting_connect() {
            tracing::info!(
                previous_state = self.state_name(),
                "session.reconnect — resetting to Creating from non-AwaitingConnect state"
            );
        }
        self.state = SessionState::Creating {
            genre_slug: genre.to_string(),
            world_slug: world.to_string(),
            player_name: player_name.to_string(),
        };

        Ok(GameMessage::SessionEvent {
            payload: sidequest_protocol::SessionEventPayload {
                event: "connected".to_string(),
                player_name: Some(player_name.to_string()),
                genre: Some(genre.to_string()),
                world: Some(world.to_string()),
                has_character: Some(false),
                initial_state: None,
                css: None,
                image_cooldown_seconds: None,
                narrator_verbosity: None,
                narrator_vocabulary: None,
            },
            player_id: String::new(),
        })
    }

    /// Complete character creation and transition to Playing.
    pub fn complete_character_creation(&mut self) -> Result<(), ServerError> {
        match &self.state {
            SessionState::Creating {
                genre_slug,
                world_slug,
                player_name,
            } => {
                let genre_slug = genre_slug.clone();
                let world_slug = world_slug.clone();
                let player_name = player_name.clone();
                self.state = SessionState::Playing {
                    genre_slug,
                    world_slug,
                    player_name,
                };
                Ok(())
            }
            _ => Err(ServerError::Deserialization(
                "Cannot complete character creation: not in Creating state".to_string(),
            )),
        }
    }

    /// Check if the session is in AwaitingConnect state.
    pub fn is_awaiting_connect(&self) -> bool {
        matches!(self.state, SessionState::AwaitingConnect)
    }

    /// Check if the session is in Creating state.
    pub fn is_creating(&self) -> bool {
        matches!(self.state, SessionState::Creating { .. })
    }

    /// Check if the session is in Playing state.
    pub fn is_playing(&self) -> bool {
        matches!(self.state, SessionState::Playing { .. })
    }

    /// Get the current state name as a string.
    pub fn state_name(&self) -> &str {
        match &self.state {
            SessionState::AwaitingConnect => "AwaitingConnect",
            SessionState::Creating { .. } => "Creating",
            SessionState::Playing { .. } => "Playing",
        }
    }

    /// Check if a message type is valid for the current session state.
    pub fn can_handle_message_type(&self, msg_type: &str) -> bool {
        match &self.state {
            SessionState::AwaitingConnect => matches!(msg_type, "SESSION_EVENT"),
            SessionState::Creating { .. } => {
                matches!(msg_type, "CHARACTER_CREATION" | "SESSION_EVENT")
            }
            SessionState::Playing { .. } => {
                matches!(msg_type, "PLAYER_ACTION" | "SESSION_EVENT")
            }
        }
    }

    /// Reset the session to AwaitingConnect state. Used on disconnect.
    pub fn cleanup(&mut self) {
        self.state = SessionState::AwaitingConnect;
    }

    /// Get the bound genre slug, if connected.
    pub fn genre_slug(&self) -> Option<&str> {
        match &self.state {
            SessionState::Creating { genre_slug, .. }
            | SessionState::Playing { genre_slug, .. } => Some(genre_slug),
            _ => None,
        }
    }

    /// Get the bound world slug, if connected.
    pub fn world_slug(&self) -> Option<&str> {
        match &self.state {
            SessionState::Creating { world_slug, .. }
            | SessionState::Playing { world_slug, .. } => Some(world_slug),
            _ => None,
        }
    }

    /// Get the player name, if connected.
    pub fn player_name(&self) -> Option<&str> {
        match &self.state {
            SessionState::Creating { player_name, .. }
            | SessionState::Playing { player_name, .. } => Some(player_name),
            _ => None,
        }
    }
}

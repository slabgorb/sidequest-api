//! Session management — wraps GameStore with active session tracking.
//!
//! SessionManager owns the current game session and delegates persistence
//! to GameStore.

use crate::persistence::{GameStore, PersistenceError};
use crate::state::GameSnapshot;

// Re-export SaveInfo so tests can import from session module
pub use crate::persistence::SaveInfo;

/// Manages the lifecycle of a game session.
///
/// Wraps a GameStore and tracks the currently active session.
pub struct SessionManager {
    store: GameStore,
    active_snapshot: Option<GameSnapshot>,
    active_save_id: Option<i64>,
}

impl SessionManager {
    /// Create a new session manager with the given store.
    pub fn new(store: GameStore) -> Self {
        Self {
            store,
            active_snapshot: None,
            active_save_id: None,
        }
    }

    /// Start a new session with the given snapshot.
    pub fn start_session(&mut self, snapshot: GameSnapshot) -> Result<(), PersistenceError> {
        self.active_snapshot = Some(snapshot);
        self.active_save_id = None;
        Ok(())
    }

    /// Whether there is an active session.
    pub fn has_active_session(&self) -> bool {
        self.active_snapshot.is_some()
    }

    /// Save the current session to the store. Returns the save ID.
    pub fn save(&mut self) -> Result<i64, PersistenceError> {
        let snapshot = self
            .active_snapshot
            .as_ref()
            .ok_or_else(|| {
                PersistenceError::Database(rusqlite::Error::InvalidParameterName(
                    "no active session".to_string(),
                ))
            })?;

        let save_id = if let Some(existing_id) = self.active_save_id {
            self.store.auto_save(existing_id, snapshot)?;
            existing_id
        } else {
            let id = self.store.save(snapshot)?;
            self.active_save_id = Some(id);
            id
        };

        Ok(save_id)
    }
}

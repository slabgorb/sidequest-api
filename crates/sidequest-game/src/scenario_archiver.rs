//! Scenario archiver — versioned save/resume for mid-scenario state.
//!
//! Story 7-7: Wraps `ScenarioState` in a `VersionedScenario` for format
//! evolution, delegates raw storage to `SessionStore::save_scenario()` /
//! `load_scenario()`. Version mismatches reject cleanly rather than producing
//! undefined behavior from deserialized stale state.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::persistence::{PersistError, SessionStore};
use crate::scenario_state::ScenarioState;

/// Current scenario archive format version. Bump when ScenarioState fields
/// change in a backward-incompatible way.
pub const SCENARIO_FORMAT_VERSION: u32 = 1;

/// Version-tagged wrapper around ScenarioState for format evolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedScenario {
    /// Format version — checked on load, set to SCENARIO_FORMAT_VERSION on save.
    pub version: u32,
    /// The scenario state payload.
    pub state: ScenarioState,
}

/// Errors from scenario archive operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ArchiveError {
    /// Stored scenario has a different format version than expected.
    #[error("scenario version mismatch: expected {expected}, found {found}")]
    VersionMismatch {
        /// The version this build expects.
        expected: u32,
        /// The version found in the stored data.
        found: u32,
    },
    /// Underlying persistence error.
    #[error("store error: {0}")]
    Store(#[from] PersistError),
    /// JSON serialization/deserialization failure.
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Versioned persistence for scenario state.
///
/// Wraps `ScenarioState` in `VersionedScenario` on save, checks version on
/// load. Delegates raw JSON storage to `SessionStore`.
pub struct ScenarioArchiver {
    store: Arc<dyn SessionStore>,
}

impl ScenarioArchiver {
    /// Create a new archiver backed by the given session store.
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        Self { store }
    }

    /// Save scenario state with version tagging.
    pub fn save(&self, session_id: &str, state: &ScenarioState) -> Result<(), ArchiveError> {
        let versioned = VersionedScenario {
            version: SCENARIO_FORMAT_VERSION,
            state: state.clone(),
        };
        let json = serde_json::to_string(&versioned)
            .map_err(|e| ArchiveError::Serialization(e.to_string()))?;
        self.store.save_scenario(session_id, &json)?;
        Ok(())
    }

    /// Load scenario state, checking version compatibility.
    ///
    /// Returns `Ok(None)` if no scenario has been saved for this session.
    /// Returns `Err(ArchiveError::VersionMismatch)` if the stored version
    /// doesn't match `SCENARIO_FORMAT_VERSION`.
    pub fn load(&self, session_id: &str) -> Result<Option<ScenarioState>, ArchiveError> {
        let json = self.store.load_scenario(session_id)?;
        match json {
            None => Ok(None),
            Some(data) => {
                let versioned: VersionedScenario = serde_json::from_str(&data)
                    .map_err(|e| ArchiveError::Serialization(e.to_string()))?;
                if versioned.version != SCENARIO_FORMAT_VERSION {
                    return Err(ArchiveError::VersionMismatch {
                        expected: SCENARIO_FORMAT_VERSION,
                        found: versioned.version,
                    });
                }
                Ok(Some(versioned.state))
            }
        }
    }
}

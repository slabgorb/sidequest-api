//! Session persistence — rusqlite save/load/list, narrative log.
//!
//! ADR-006: game_saves table + narrative_log table.
//! ADR-023: Auto-save after every turn, atomic writes via SQLite transactions.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use thiserror::Error;

use crate::narrative::NarrativeEntry;
use crate::state::GameSnapshot;

/// Prepare a snapshot for persistence: stamp last_saved_at and serialize.
fn prepare_for_save(snapshot: &GameSnapshot) -> (GameSnapshot, String, String) {
    let now = Utc::now();
    let mut snap = snapshot.clone();
    snap.last_saved_at = Some(now);
    let state_json = serde_json::to_string(&snap).unwrap_or_default();
    let now_str = now.to_rfc3339();
    (snap, state_json, now_str)
}

/// Parse an RFC3339 timestamp, falling back to now on error.
fn parse_rfc3339_or_now(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// Errors from persistence operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PersistenceError {
    /// SQLite error.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    /// JSON serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    /// Save not found.
    #[error("save not found: {0}")]
    NotFound(i64),
}

/// Persistent game store backed by SQLite.
///
/// ADR-006 schema: game_saves + narrative_log tables.
pub struct GameStore {
    conn: Connection,
}

impl GameStore {
    /// Create an in-memory store (for testing).
    pub fn in_memory() -> Result<Self, PersistenceError> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    /// Create a file-backed store.
    pub fn open(path: &str) -> Result<Self, PersistenceError> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), PersistenceError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS game_saves (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                genre_slug TEXT NOT NULL,
                world_slug TEXT NOT NULL,
                state_json TEXT NOT NULL,
                metadata TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS narrative_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                save_id INTEGER NOT NULL,
                turn INTEGER NOT NULL,
                agent TEXT NOT NULL,
                input TEXT NOT NULL,
                response TEXT NOT NULL,
                location TEXT,
                timestamp INTEGER NOT NULL,
                tags TEXT,
                FOREIGN KEY (save_id) REFERENCES game_saves(id)
            );",
        )?;
        Ok(())
    }

    /// Save a game snapshot. Sets `last_saved_at` on the snapshot before persisting.
    /// Returns the save ID.
    pub fn save(&self, snapshot: &GameSnapshot) -> Result<i64, PersistenceError> {
        let (snap, state_json, now_str) = prepare_for_save(snapshot);

        self.conn.execute(
            "INSERT INTO game_saves (genre_slug, world_slug, state_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                snap.genre_slug,
                snap.world_slug,
                state_json,
                now_str,
                now_str
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Load a game snapshot by save ID.
    pub fn load(&self, save_id: i64) -> Result<GameSnapshot, PersistenceError> {
        let state_json: String = self
            .conn
            .query_row(
                "SELECT state_json FROM game_saves WHERE id = ?1",
                params![save_id],
                |row| row.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => PersistenceError::NotFound(save_id),
                other => PersistenceError::Database(other),
            })?;
        let snapshot: GameSnapshot = serde_json::from_str(&state_json)?;
        Ok(snapshot)
    }

    /// List all saves as SaveInfo entries.
    pub fn list_saves(&self) -> Result<Vec<SaveInfo>, PersistenceError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, genre_slug, world_slug, created_at, updated_at FROM game_saves ORDER BY id",
        )?;
        let saves = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let genre_slug: String = row.get(1)?;
                let world_slug: String = row.get(2)?;
                let created_at_str: String = row.get(3)?;
                let updated_at_str: String = row.get(4)?;
                Ok(SaveInfo {
                    save_id: id,
                    genre_slug,
                    world_slug,
                    created_at: parse_rfc3339_or_now(&created_at_str),
                    updated_at: parse_rfc3339_or_now(&updated_at_str),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(saves)
    }

    /// Auto-save: atomically update an existing save using a transaction.
    /// ADR-023: Atomic writes prevent corruption from interrupted saves.
    pub fn auto_save(&self, save_id: i64, snapshot: &GameSnapshot) -> Result<(), PersistenceError> {
        let (_snap, state_json, now_str) = prepare_for_save(snapshot);

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE game_saves SET state_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![state_json, now_str, save_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Append a narrative entry to the log for a save.
    pub fn append_narrative(
        &self,
        save_id: i64,
        entry: &NarrativeEntry,
    ) -> Result<(), PersistenceError> {
        let tags_json = serde_json::to_string(&entry.tags)?;
        self.conn.execute(
            "INSERT INTO narrative_log (save_id, turn, agent, input, response, timestamp, tags)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                save_id,
                entry.round,
                entry.author,
                "", // input field — narrative entries don't have separate input
                entry.content,
                entry.timestamp,
                tags_json,
            ],
        )?;
        Ok(())
    }

    /// Load the narrative log for a save, ordered by insertion.
    pub fn load_narrative(&self, save_id: i64) -> Result<Vec<NarrativeEntry>, PersistenceError> {
        let mut stmt = self.conn.prepare(
            "SELECT turn, agent, response, timestamp, tags
             FROM narrative_log WHERE save_id = ?1 ORDER BY id",
        )?;
        let entries = stmt
            .query_map(params![save_id], |row| {
                let round: u32 = row.get(0)?;
                let author: String = row.get(1)?;
                let content: String = row.get(2)?;
                let timestamp: u64 = row.get(3)?;
                let tags_json: String = row.get(4)?;
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                Ok(NarrativeEntry {
                    timestamp,
                    round,
                    author,
                    content,
                    tags,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }
}

/// Metadata about a saved game (returned by list_saves).
pub struct SaveInfo {
    save_id: i64,
    genre_slug: String,
    world_slug: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl SaveInfo {
    /// Create a SaveInfo from a snapshot and save ID.
    pub fn from_snapshot(snapshot: &GameSnapshot, save_id: i64) -> Self {
        let now = Utc::now();
        Self {
            save_id,
            genre_slug: snapshot.genre_slug.clone(),
            world_slug: snapshot.world_slug.clone(),
            created_at: now,
            updated_at: now,
        }
    }

    /// The save ID.
    pub fn save_id(&self) -> i64 {
        self.save_id
    }

    /// The genre slug.
    pub fn genre_slug(&self) -> &str {
        &self.genre_slug
    }

    /// The world slug.
    pub fn world_slug(&self) -> &str {
        &self.world_slug
    }

    /// When the save was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// When the save was last updated.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

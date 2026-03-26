//! Session persistence — rusqlite save/load/list, narrative log.
//!
//! ADR-006: game_saves table + narrative_log table.
//! ADR-023: Auto-save after every turn, atomic writes via SQLite transactions.

use std::path::Path;

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

// ============================================================================
// Story 2-4: New SessionStore trait + SqliteStore implementation
// ============================================================================

/// Errors from persistence operations (Story 2-4).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PersistError {
    /// Database error.
    #[error("database error: {0}")]
    Database(String),
    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),
    /// Save not found.
    #[error("save not found")]
    NotFound,
}

impl From<rusqlite::Error> for PersistError {
    fn from(e: rusqlite::Error) -> Self {
        PersistError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for PersistError {
    fn from(e: serde_json::Error) -> Self {
        PersistError::Serialization(e.to_string())
    }
}

/// A loaded session: metadata + game state + optional recap.
pub struct SavedSession {
    /// Session metadata.
    pub meta: SessionMeta,
    /// The game state snapshot.
    pub snapshot: GameSnapshot,
    /// "Previously On..." recap, or None for fresh games.
    pub recap: Option<String>,
}

/// Session metadata stored in the session_meta table.
pub struct SessionMeta {
    /// Genre pack slug.
    pub genre_slug: String,
    /// World slug.
    pub world_slug: String,
    /// When the session was first created.
    pub created_at: DateTime<Utc>,
    /// When the session was last played.
    pub last_played: DateTime<Utc>,
}

/// Persistence contract — the server depends on this trait, not rusqlite directly.
pub trait SessionStore {
    /// Save the current game state.
    fn save(&self, snapshot: &GameSnapshot) -> Result<(), PersistError>;
    /// Load the saved session, or None if no save exists.
    fn load(&self) -> Result<Option<SavedSession>, PersistError>;
    /// Append a narrative entry to the log.
    fn append_narrative(&self, entry: &NarrativeEntry) -> Result<(), PersistError>;
    /// Get the most recent narrative entries, ordered oldest-first.
    fn recent_narrative(&self, limit: usize) -> Result<Vec<NarrativeEntry>, PersistError>;
    /// Generate a "Previously On..." recap from recent entries.
    fn generate_recap(&self) -> Result<Option<String>, PersistError>;
}

/// SQLite-backed session store. One .db file per save slot.
///
/// Uses singleton tables (session_meta, game_state) plus append-only narrative_log.
/// Connection is `!Send` — each session actor owns its own store (ADR-003).
pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    /// Open an in-memory store (for testing).
    pub fn open_in_memory() -> Result<Self, PersistError> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    /// Open a file-backed store.
    pub fn open(path: &str) -> Result<Self, PersistError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    /// Initialize session metadata (genre + world). Call once when creating a new session.
    pub fn init_session(&self, genre_slug: &str, world_slug: &str) -> Result<(), PersistError> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT OR REPLACE INTO session_meta (id, genre_slug, world_slug, created_at, last_played, schema_version)
             VALUES (1, ?1, ?2, ?3, ?4, 1)",
            params![genre_slug, world_slug, now, now],
        )?;
        Ok(())
    }

    /// Scan a directory tree for save.db files. Returns SaveInfo for each found.
    pub fn list_saves(root: &Path) -> Result<Vec<SaveListEntry>, PersistError> {
        let mut saves = Vec::new();

        if !root.exists() {
            return Ok(saves);
        }

        // Walk genre/world/save.db structure
        let genre_dirs =
            std::fs::read_dir(root).map_err(|e| PersistError::Database(e.to_string()))?;

        for genre_entry in genre_dirs.flatten() {
            let genre_path = genre_entry.path();
            if !genre_path.is_dir() {
                continue;
            }
            let genre_slug = match genre_path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            let world_dirs = match std::fs::read_dir(&genre_path) {
                Ok(d) => d,
                Err(_) => continue,
            };

            for world_entry in world_dirs.flatten() {
                let world_path = world_entry.path();
                if !world_path.is_dir() {
                    continue;
                }
                let world_slug = match world_path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => name.to_string(),
                    None => continue,
                };

                let db_path = world_path.join("save.db");
                if db_path.exists() {
                    // Read metadata from the save file
                    if let Ok(store) = SqliteStore::open(db_path.to_str().unwrap_or_default()) {
                        let meta = store.load_meta();
                        saves.push(SaveListEntry {
                            genre_slug: meta
                                .as_ref()
                                .map(|m| m.genre_slug.clone())
                                .unwrap_or(genre_slug.clone()),
                            world_slug: meta
                                .as_ref()
                                .map(|m| m.world_slug.clone())
                                .unwrap_or(world_slug.clone()),
                        });
                    }
                }
            }
        }

        Ok(saves)
    }

    fn init_schema(&self) -> Result<(), PersistError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS session_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                genre_slug TEXT NOT NULL,
                world_slug TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_played TEXT NOT NULL,
                schema_version INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS game_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                snapshot_json TEXT NOT NULL,
                saved_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS narrative_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                round_number INTEGER NOT NULL,
                author TEXT NOT NULL,
                content TEXT NOT NULL,
                tags TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_narrative_round ON narrative_log(round_number);
            CREATE INDEX IF NOT EXISTS idx_narrative_author ON narrative_log(author);",
        )?;
        Ok(())
    }

    fn load_meta(&self) -> Option<SessionMeta> {
        self.conn
            .query_row(
                "SELECT genre_slug, world_slug, created_at, last_played FROM session_meta WHERE id = 1",
                [],
                |row| {
                    let genre_slug: String = row.get(0)?;
                    let world_slug: String = row.get(1)?;
                    let created_at_str: String = row.get(2)?;
                    let last_played_str: String = row.get(3)?;
                    Ok(SessionMeta {
                        genre_slug,
                        world_slug,
                        created_at: parse_rfc3339_or_now(&created_at_str),
                        last_played: parse_rfc3339_or_now(&last_played_str),
                    })
                },
            )
            .ok()
    }
}

impl SessionStore for SqliteStore {
    fn save(&self, snapshot: &GameSnapshot) -> Result<(), PersistError> {
        let now = Utc::now();
        let mut snap = snapshot.clone();
        snap.last_saved_at = Some(now);
        let state_json = serde_json::to_string(&snap)?;
        let now_str = now.to_rfc3339();

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO game_state (id, snapshot_json, saved_at) VALUES (1, ?1, ?2)",
            params![state_json, now_str],
        )?;
        // Update last_played if session_meta exists
        tx.execute(
            "UPDATE session_meta SET last_played = ?1 WHERE id = 1",
            params![now_str],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn load(&self) -> Result<Option<SavedSession>, PersistError> {
        // Load game state
        let state_json: Option<String> = self
            .conn
            .query_row(
                "SELECT snapshot_json FROM game_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .ok();

        let state_json = match state_json {
            Some(json) => json,
            None => return Ok(None),
        };

        let snapshot: GameSnapshot = serde_json::from_str(&state_json)?;

        // Load meta (or synthesize from snapshot)
        let meta = self.load_meta().unwrap_or_else(|| SessionMeta {
            genre_slug: snapshot.genre_slug.clone(),
            world_slug: snapshot.world_slug.clone(),
            created_at: Utc::now(),
            last_played: Utc::now(),
        });

        // Generate recap
        let recap = self.generate_recap()?;

        Ok(Some(SavedSession {
            meta,
            snapshot,
            recap,
        }))
    }

    fn append_narrative(&self, entry: &NarrativeEntry) -> Result<(), PersistError> {
        let tags_json = serde_json::to_string(&entry.tags)?;
        self.conn.execute(
            "INSERT INTO narrative_log (round_number, author, content, tags)
             VALUES (?1, ?2, ?3, ?4)",
            params![entry.round, entry.author, entry.content, tags_json],
        )?;
        Ok(())
    }

    fn recent_narrative(&self, limit: usize) -> Result<Vec<NarrativeEntry>, PersistError> {
        // Get the last N entries by id, then return in insertion (ascending) order
        let mut stmt = self.conn.prepare(
            "SELECT round_number, author, content, tags, created_at
             FROM (SELECT * FROM narrative_log ORDER BY id DESC LIMIT ?1)
             ORDER BY id ASC",
        )?;
        let entries = stmt
            .query_map(params![limit as i64], |row| {
                let round: u32 = row.get(0)?;
                let author: String = row.get(1)?;
                let content: String = row.get(2)?;
                let tags_json: String = row.get::<_, String>(3).unwrap_or_default();
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                Ok(NarrativeEntry {
                    timestamp: 0, // not stored in new schema
                    round,
                    author,
                    content,
                    tags,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    fn generate_recap(&self) -> Result<Option<String>, PersistError> {
        let entries = self.recent_narrative(20)?;
        if entries.is_empty() {
            return Ok(None);
        }

        let mut recap = String::from("Previously On...\n\n");
        for entry in &entries {
            recap.push_str(&format!("- {}\n", entry.content));
        }
        Ok(Some(recap))
    }
}

/// Entry returned by `SqliteStore::list_saves()`.
pub struct SaveListEntry {
    /// Genre slug from the save file.
    pub genre_slug: String,
    /// World slug from the save file.
    pub world_slug: String,
}

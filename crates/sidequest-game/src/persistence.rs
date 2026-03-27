//! Session persistence — SqliteStore with actor-based PersistenceWorker.
//!
//! ADR-006: One .db file per genre/world session.
//! ADR-023: Auto-save after every turn, atomic writes via SQLite transactions.
//! ADR-003: Each session actor owns its own store (Connection is !Send).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::narrative::NarrativeEntry;
use crate::state::GameSnapshot;

/// Parse an RFC3339 timestamp, falling back to now on error.
fn parse_rfc3339_or_now(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

// ============================================================================
// Error types
// ============================================================================

/// Errors from persistence operations.
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
    /// Worker is gone (channel closed).
    #[error("persistence worker unavailable")]
    WorkerGone,
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

// ============================================================================
// SessionStore trait + data types
// ============================================================================

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

/// Entry returned by `SqliteStore::list_saves()`.
pub struct SaveListEntry {
    /// Genre slug from the save file.
    pub genre_slug: String,
    /// World slug from the save file.
    pub world_slug: String,
}

// ============================================================================
// SqliteStore — one .db file per session
// ============================================================================

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

    /// Scan a directory tree for save.db files.
    pub fn list_saves(root: &Path) -> Result<Vec<SaveListEntry>, PersistError> {
        let mut saves = Vec::new();
        if !root.exists() {
            return Ok(saves);
        }
        let genre_dirs =
            std::fs::read_dir(root).map_err(|e| PersistError::Database(e.to_string()))?;
        for genre_entry in genre_dirs.flatten() {
            let genre_path = genre_entry.path();
            if !genre_path.is_dir() { continue; }
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
                if !world_path.is_dir() { continue; }
                let world_slug = match world_path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => name.to_string(),
                    None => continue,
                };
                let db_path = world_path.join("save.db");
                if db_path.exists() {
                    if let Ok(store) = SqliteStore::open(db_path.to_str().unwrap_or_default()) {
                        let meta = store.load_meta();
                        saves.push(SaveListEntry {
                            genre_slug: meta.as_ref().map(|m| m.genre_slug.clone()).unwrap_or(genre_slug.clone()),
                            world_slug: meta.as_ref().map(|m| m.world_slug.clone()).unwrap_or(world_slug.clone()),
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
        tx.execute(
            "UPDATE session_meta SET last_played = ?1 WHERE id = 1",
            params![now_str],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn load(&self) -> Result<Option<SavedSession>, PersistError> {
        let state_json: Option<String> = self
            .conn
            .query_row("SELECT snapshot_json FROM game_state WHERE id = 1", [], |row| row.get(0))
            .ok();
        let state_json = match state_json {
            Some(json) => json,
            None => return Ok(None),
        };
        let snapshot: GameSnapshot = serde_json::from_str(&state_json)?;
        let meta = self.load_meta().unwrap_or_else(|| SessionMeta {
            genre_slug: snapshot.genre_slug.clone(),
            world_slug: snapshot.world_slug.clone(),
            created_at: Utc::now(),
            last_played: Utc::now(),
        });
        let recap = self.generate_recap()?;
        Ok(Some(SavedSession { meta, snapshot, recap }))
    }

    fn append_narrative(&self, entry: &NarrativeEntry) -> Result<(), PersistError> {
        let tags_json = serde_json::to_string(&entry.tags)?;
        self.conn.execute(
            "INSERT INTO narrative_log (round_number, author, content, tags) VALUES (?1, ?2, ?3, ?4)",
            params![entry.round, entry.author, entry.content, tags_json],
        )?;
        Ok(())
    }

    fn recent_narrative(&self, limit: usize) -> Result<Vec<NarrativeEntry>, PersistError> {
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
                Ok(NarrativeEntry { timestamp: 0, round, author, content, tags })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    fn generate_recap(&self) -> Result<Option<String>, PersistError> {
        let entries = self.recent_narrative(20)?;
        if entries.is_empty() { return Ok(None); }
        let mut recap = String::from("Previously On...\n\n");
        for entry in &entries {
            recap.push_str(&format!("- {}\n", entry.content));
        }
        Ok(Some(recap))
    }
}

// ============================================================================
// PersistenceWorker — actor pattern for !Send SqliteStore
// ============================================================================

/// Commands sent to the persistence worker over mpsc.
pub enum PersistenceCommand {
    /// Save a game snapshot.
    Save {
        /// Genre slug for the session.
        genre_slug: String,
        /// World slug for the session.
        world_slug: String,
        /// The game state to persist.
        snapshot: GameSnapshot,
        /// Reply channel.
        reply: oneshot::Sender<Result<(), PersistError>>,
    },
    /// Load a saved session.
    Load {
        /// Genre slug for the session.
        genre_slug: String,
        /// World slug for the session.
        world_slug: String,
        /// Reply channel.
        reply: oneshot::Sender<Result<Option<SavedSession>, PersistError>>,
    },
    /// Append a narrative entry.
    AppendNarrative {
        /// Genre slug for the session.
        genre_slug: String,
        /// World slug for the session.
        world_slug: String,
        /// The narrative entry to append.
        entry: NarrativeEntry,
        /// Reply channel.
        reply: oneshot::Sender<Result<(), PersistError>>,
    },
    /// Check if a save exists on disk.
    Exists {
        /// Genre slug for the session.
        genre_slug: String,
        /// World slug for the session.
        world_slug: String,
        /// Reply channel.
        reply: oneshot::Sender<bool>,
    },
    /// List all saved sessions.
    ListSaves {
        /// Reply channel.
        reply: oneshot::Sender<Result<Vec<SaveListEntry>, PersistError>>,
    },
    /// Graceful shutdown.
    Shutdown,
}

/// Clone + Send + Sync handle for persistence operations.
///
/// All methods are async — the blocking SQLite work runs on the worker's dedicated thread.
#[derive(Clone)]
pub struct PersistenceHandle {
    tx: mpsc::Sender<PersistenceCommand>,
}

impl PersistenceHandle {
    /// Save a game snapshot for a genre/world session.
    #[tracing::instrument(skip(self, snapshot), fields(genre = %genre_slug, world = %world_slug))]
    pub async fn save(&self, genre_slug: &str, world_slug: &str, snapshot: &GameSnapshot) -> Result<(), PersistError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx.send(PersistenceCommand::Save {
            genre_slug: genre_slug.to_string(),
            world_slug: world_slug.to_string(),
            snapshot: snapshot.clone(),
            reply: reply_tx,
        }).await.map_err(|_| PersistError::WorkerGone)?;
        reply_rx.await.map_err(|_| PersistError::WorkerGone)?
    }

    /// Load a saved session, or None if no save exists.
    #[tracing::instrument(skip(self), fields(genre = %genre_slug, world = %world_slug))]
    pub async fn load(&self, genre_slug: &str, world_slug: &str) -> Result<Option<SavedSession>, PersistError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx.send(PersistenceCommand::Load {
            genre_slug: genre_slug.to_string(),
            world_slug: world_slug.to_string(),
            reply: reply_tx,
        }).await.map_err(|_| PersistError::WorkerGone)?;
        reply_rx.await.map_err(|_| PersistError::WorkerGone)?
    }

    /// Append a narrative entry to a session's log.
    #[tracing::instrument(skip(self, entry), fields(genre = %genre_slug, world = %world_slug))]
    pub async fn append_narrative(&self, genre_slug: &str, world_slug: &str, entry: &NarrativeEntry) -> Result<(), PersistError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx.send(PersistenceCommand::AppendNarrative {
            genre_slug: genre_slug.to_string(),
            world_slug: world_slug.to_string(),
            entry: entry.clone(),
            reply: reply_tx,
        }).await.map_err(|_| PersistError::WorkerGone)?;
        reply_rx.await.map_err(|_| PersistError::WorkerGone)?
    }

    /// Check if a save exists on disk for a genre/world pair.
    #[tracing::instrument(skip(self), fields(genre = %genre_slug, world = %world_slug))]
    pub async fn exists(&self, genre_slug: &str, world_slug: &str) -> bool {
        let (reply_tx, reply_rx) = oneshot::channel();
        let sent = self.tx.send(PersistenceCommand::Exists {
            genre_slug: genre_slug.to_string(),
            world_slug: world_slug.to_string(),
            reply: reply_tx,
        }).await;
        if sent.is_err() { return false; }
        reply_rx.await.unwrap_or(false)
    }

    /// List all saved sessions under the save directory.
    #[tracing::instrument(skip(self))]
    pub async fn list_saves(&self) -> Result<Vec<SaveListEntry>, PersistError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx.send(PersistenceCommand::ListSaves { reply: reply_tx }).await.map_err(|_| PersistError::WorkerGone)?;
        reply_rx.await.map_err(|_| PersistError::WorkerGone)?
    }

    /// Signal the worker to shut down gracefully.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(PersistenceCommand::Shutdown).await;
    }
}

/// Dedicated thread that owns SqliteStore connections and processes commands.
///
/// Spawned via `std::thread::spawn` because `rusqlite::Connection` is `!Send`.
/// Uses `HashMap<String, SqliteStore>` to cache open stores by genre/world key.
pub struct PersistenceWorker {
    save_dir: PathBuf,
    stores: HashMap<String, SqliteStore>,
    rx: mpsc::Receiver<PersistenceCommand>,
}

impl PersistenceWorker {
    /// Spawn the persistence worker on a dedicated OS thread.
    /// Returns a `PersistenceHandle` for async callers.
    pub fn spawn(save_dir: PathBuf) -> PersistenceHandle {
        let (tx, rx) = mpsc::channel::<PersistenceCommand>(256);
        let handle = PersistenceHandle { tx };
        let rt_handle = tokio::runtime::Handle::current();
        std::thread::Builder::new()
            .name("persistence-worker".into())
            .spawn(move || {
                let mut worker = PersistenceWorker {
                    save_dir,
                    stores: HashMap::new(),
                    rx,
                };
                worker.run(rt_handle);
            })
            .expect("failed to spawn persistence worker thread");
        handle
    }

    fn run(&mut self, rt: tokio::runtime::Handle) {
        tracing::info!(save_dir = %self.save_dir.display(), "Persistence worker started");
        loop {
            let cmd = match rt.block_on(self.rx.recv()) {
                Some(cmd) => cmd,
                None => {
                    tracing::info!("Persistence worker: channel closed, exiting");
                    break;
                }
            };
            match cmd {
                PersistenceCommand::Shutdown => {
                    tracing::info!("Persistence worker: shutdown requested");
                    break;
                }
                other => self.handle_command(other),
            }
        }
        self.stores.clear();
        tracing::info!("Persistence worker stopped");
    }

    fn store_key(genre_slug: &str, world_slug: &str) -> String {
        format!("{}/{}", genre_slug, world_slug)
    }

    fn db_path(&self, genre_slug: &str, world_slug: &str) -> PathBuf {
        self.save_dir.join(genre_slug).join(world_slug).join("save.db")
    }

    fn get_or_open_store(&mut self, genre_slug: &str, world_slug: &str) -> Result<&SqliteStore, PersistError> {
        let key = Self::store_key(genre_slug, world_slug);
        if !self.stores.contains_key(&key) {
            let db_path = self.db_path(genre_slug, world_slug);
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| PersistError::Database(format!("mkdir failed: {}", e)))?;
            }
            let store = SqliteStore::open(db_path.to_str().unwrap_or_default())?;
            store.init_session(genre_slug, world_slug)?;
            tracing::info!(genre = %genre_slug, world = %world_slug, "Session store opened");
            self.stores.insert(key.clone(), store);
        }
        Ok(self.stores.get(&key).unwrap())
    }

    fn handle_command(&mut self, cmd: PersistenceCommand) {
        match cmd {
            PersistenceCommand::Save { genre_slug, world_slug, snapshot, reply } => {
                let _span = tracing::info_span!("persistence_save", genre = %genre_slug, world = %world_slug).entered();
                let result = self.get_or_open_store(&genre_slug, &world_slug).and_then(|store| store.save(&snapshot));
                match &result {
                    Ok(()) => tracing::info!("Session saved"),
                    Err(e) => tracing::warn!(error = %e, "Save failed"),
                }
                let _ = reply.send(result);
            }
            PersistenceCommand::Load { genre_slug, world_slug, reply } => {
                let _span = tracing::info_span!("persistence_load", genre = %genre_slug, world = %world_slug).entered();
                let result = self.get_or_open_store(&genre_slug, &world_slug).and_then(|store| store.load());
                match &result {
                    Ok(Some(_)) => tracing::info!("Session loaded"),
                    Ok(None) => tracing::debug!("No saved session found"),
                    Err(e) => tracing::warn!(error = %e, "Load failed"),
                }
                let _ = reply.send(result);
            }
            PersistenceCommand::AppendNarrative { genre_slug, world_slug, entry, reply } => {
                let result = self.get_or_open_store(&genre_slug, &world_slug).and_then(|store| store.append_narrative(&entry));
                let _ = reply.send(result);
            }
            PersistenceCommand::Exists { genre_slug, world_slug, reply } => {
                let db_path = self.db_path(&genre_slug, &world_slug);
                let exists = db_path.exists();
                tracing::debug!(genre = %genre_slug, world = %world_slug, exists, "Checking save");
                let _ = reply.send(exists);
            }
            PersistenceCommand::ListSaves { reply } => {
                let result = SqliteStore::list_saves(&self.save_dir);
                let _ = reply.send(result);
            }
            PersistenceCommand::Shutdown => unreachable!("handled in run loop"),
        }
    }
}

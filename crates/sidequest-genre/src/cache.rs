//! Genre pack cache — load once, return same `Arc<GenrePack>` for same code.

use crate::error::GenreError;
use crate::genre_code::GenreCode;
use crate::loader::GenreLoader;
use crate::models::GenrePack;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Thread-safe cache for loaded genre packs.
///
/// Returns the same `Arc<GenrePack>` for repeated loads of the same genre code.
pub struct GenreCache {
    packs: Mutex<HashMap<String, Arc<GenrePack>>>,
}

impl GenreCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self {
            packs: Mutex::new(HashMap::new()),
        }
    }

    /// Get a cached pack or load it via the loader.
    ///
    /// If the genre code has been loaded before, returns the same `Arc`.
    /// Otherwise, loads from disk and caches the result.
    pub fn get_or_load(
        &self,
        code: &GenreCode,
        loader: &GenreLoader,
    ) -> Result<Arc<GenrePack>, GenreError> {
        let mut cache = self.packs.lock().unwrap();
        if let Some(pack) = cache.get(code.as_str()) {
            return Ok(Arc::clone(pack));
        }
        let pack = Arc::new(loader.load(code)?);
        cache.insert(code.as_str().to_string(), Arc::clone(&pack));
        Ok(pack)
    }
}

impl Default for GenreCache {
    fn default() -> Self {
        Self::new()
    }
}

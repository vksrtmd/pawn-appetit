//! What position search keeps between calls, tied to the database file it
//! came from: the game rows read for an in-memory scan and the finished
//! search results.
//!
//! [`invalidate`] drops both for one file and is called by every write to it
//! (`write::apply`, `write::drop_database`). Each file has a generation that
//! `invalidate` bumps: a search notes it before reading and stores under it,
//! so a search that read the file before a write cannot put its stale rows
//! back after the write cleared them.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
};

use super::{models::NormalizedGame, GameQueryJs, PositionStats};
use crate::GameData;

pub type SearchResult = (Vec<PositionStats>, Vec<NormalizedGame>);

const RESULT_CAPACITY: usize = 100;
/// Files whose rows are kept in memory at once. Tests run many files in
/// parallel against the one cache.
const ROWS_CAPACITY: usize = if cfg!(test) { 64 } else { 3 };

pub struct SearchCache {
    inner: Mutex<Inner>,
}

struct Inner {
    generations: HashMap<PathBuf, u64>,
    rows: HashMap<PathBuf, Arc<Vec<GameData>>>,
    results: lru::LruCache<(GameQueryJs, PathBuf), SearchResult>,
}

/// The process-wide cache every search and write goes through.
pub static SEARCH_CACHE: LazyLock<SearchCache> = LazyLock::new(SearchCache::new);

/// Drops everything derived from the games of `path`.
pub fn invalidate(path: &Path) {
    SEARCH_CACHE.invalidate(path);
}

/// One key per file however it was spelled.
fn key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

impl SearchCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                generations: HashMap::new(),
                rows: HashMap::new(),
                results: lru::LruCache::new(std::num::NonZeroUsize::new(RESULT_CAPACITY).unwrap()),
            }),
        }
    }

    /// Note this before reading the file; hand it back when storing.
    pub fn generation(&self, path: &Path) -> u64 {
        let inner = self.inner.lock().unwrap();
        inner.generations.get(&key(path)).copied().unwrap_or(0)
    }

    pub fn rows(&self, path: &Path) -> Option<Arc<Vec<GameData>>> {
        self.inner.lock().unwrap().rows.get(&key(path)).cloned()
    }

    pub fn store_rows(&self, path: &Path, generation: u64, rows: Arc<Vec<GameData>>) {
        let path = key(path);
        let mut inner = self.inner.lock().unwrap();
        if inner.generations.get(&path).copied().unwrap_or(0) != generation {
            return;
        }
        if inner.rows.len() >= ROWS_CAPACITY && !inner.rows.contains_key(&path) {
            inner.rows.clear();
        }
        inner.rows.insert(path, rows);
    }

    pub fn result(&self, query: &GameQueryJs, path: &Path) -> Option<SearchResult> {
        let k = (query.clone(), key(path));
        self.inner.lock().unwrap().results.get(&k).cloned()
    }

    pub fn store_result(
        &self,
        query: &GameQueryJs,
        path: &Path,
        generation: u64,
        result: SearchResult,
    ) {
        let path = key(path);
        let mut inner = self.inner.lock().unwrap();
        if inner.generations.get(&path).copied().unwrap_or(0) == generation {
            inner.results.push((query.clone(), path), result);
        }
    }

    pub fn invalidate(&self, path: &Path) {
        let path = key(path);
        let mut inner = self.inner.lock().unwrap();
        *inner.generations.entry(path.clone()).or_insert(0) += 1;
        inner.rows.remove(&path);
        let stale: Vec<_> = inner
            .results
            .iter()
            .filter(|((_, p), _)| *p == path)
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            inner.results.pop(&k);
        }
    }

    /// Forgets every file's rows and results.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        for g in inner.generations.values_mut() {
            *g += 1;
        }
        inner.rows.clear();
        inner.results.clear();
    }
}

impl Default for SearchCache {
    fn default() -> Self {
        Self::new()
    }
}
